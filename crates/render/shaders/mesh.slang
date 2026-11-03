// Forward uber-shaders: one vertex shader pulling skinned vertices, fragment shaders for PBR, Toon and Unlit
// (design doc §6, ADR 0023). Compiled by `cargo xtask shaders`.
//
// Nothing is bound per draw: the vertex shader finds its draw record through the instance index (the indirect
// command's `firstInstance`), and from it the material, the skinned vertices and the static attributes. Buffers
// are reached by device address, textures through one bindless array.

static const uint NONE = 0xffffffff;
static const float PI = 3.14159265;

struct Skinned
{
    float4 position;
    float4 normal;
    float4 tangent; // w: handedness
};

// One draw: its material and where its mesh's vertices start in the shared buffers.
struct Draw
{
    uint material;
    uint skinned; // first vertex in the skinned buffer
    uint attrs;   // first vertex in the attribute buffer
    uint pad;
};

// A texture slot: `st` is (scale u, scale v, offset u, offset v); `tex` is (texture or NONE, uv set, 0, 0).
struct Slot
{
    float4 st;
    uint4 tex;
};

struct Material
{
    float4 base_color;
    float4 emissive; // xyz
    float4 shade;    // xyz, toon shadow colour
    float4 rim;      // xyz, toon rim colour
    float4 params;   // metallic, roughness, normal scale, alpha cutoff (0: no cutoff)
    float4 toon;     // shade shift, shade toony, rim power, matcap strength
    Slot slots[6];   // base, normal, metallic-roughness or shade, occlusion, emissive, matcap
};

struct View
{
    float4 view_proj[4]; // columns
    float4 eye;
    float4 light_dir;    // towards the light
    float4 light_color;  // what a white Lambert surface facing the light shows
    float4 ambient;
};

struct Push
{
    Skinned* skinned;
    float4* attrs; // uv0 in xy, uv1 in zw
    Material* materials;
    Draw* draws;
    View* view;
};

[[vk::push_constant]] Push pc;

[[vk::binding(0, 0)]] Sampler2D textures[];

struct Varyings
{
    float4 position : SV_Position;
    float3 world : WORLD;
    float3 normal : NORMAL;
    float4 tangent : TANGENT;
    float4 uv : TEXCOORD;
    nointerpolation uint material : MATERIAL;
};

[shader("vertex")]
Varyings vertex(uint index: SV_VulkanVertexID, uint instance: SV_VulkanInstanceID)
{
    Draw d = pc.draws[instance];
    Skinned s = pc.skinned[d.skinned + index];
    View v = *pc.view;
    float3 p = s.position.xyz;
    Varyings o;
    o.position = v.view_proj[0] * p.x + v.view_proj[1] * p.y + v.view_proj[2] * p.z + v.view_proj[3];
    o.world = p;
    o.normal = s.normal.xyz;
    o.tangent = s.tangent;
    o.uv = pc.attrs[d.attrs + index];
    o.material = d.material;
    return o;
}

// The slot's texel, or white without a texture.
float4 sample(Slot s, float4 uv)
{
    if (s.tex.x == NONE)
        return 1;
    float2 c = (s.tex.y == 0 ? uv.xy : uv.zw) * s.st.xy + s.st.zw;
    return textures[NonUniformResourceIndex(s.tex.x)].Sample(c);
}

// Base colour times its texture; discards below the alpha cutoff.
float4 base(Material m, float4 uv)
{
    float4 c = m.base_color * sample(m.slots[0], uv);
    if (m.params.w > 0 && c.a < m.params.w)
        discard;
    return c;
}

// The shading normal: the interpolated normal, perturbed by the normal map if there is one and a tangent.
float3 normal(Material m, Varyings i, bool front)
{
    float3 n = normalize(i.normal);
    if (!front)
        n = -n;
    if (m.slots[1].tex.x == NONE || dot(i.tangent.xyz, i.tangent.xyz) == 0)
        return n;
    float3 t = normalize(i.tangent.xyz - n * dot(n, i.tangent.xyz));
    float3 b = cross(n, t) * i.tangent.w;
    float3 tn = sample(m.slots[1], i.uv).xyz * 2 - 1;
    tn.xy *= m.params.z;
    return normalize(t * tn.x + b * tn.y + n * tn.z);
}

[shader("fragment")]
float4 pbr(Varyings i, bool front: SV_IsFrontFace) : SV_Target
{
    Material m = pc.materials[i.material];
    View v = *pc.view;
    float4 c = base(m, i.uv);
    float3 n = normal(m, i, front);
    float3 l = v.light_dir.xyz;
    float3 e = normalize(v.eye.xyz - i.world);
    float3 h = normalize(l + e);
    float4 mr = sample(m.slots[2], i.uv);
    float metallic = m.params.x * mr.b;
    float roughness = clamp(m.params.y * mr.g, 0.03, 1);
    float occlusion = m.slots[3].tex.x == NONE ? 1 : sample(m.slots[3], i.uv).r;
    float nl = saturate(dot(n, l));
    float nv = max(dot(n, e), 1e-4);
    float nh = saturate(dot(n, h));
    float a2 = roughness * roughness * roughness * roughness;
    float dd = nh * nh * (a2 - 1) + 1;
    float distribution = a2 / (PI * dd * dd);
    float visibility = 0.5 / (nl * sqrt(nv * nv * (1 - a2) + a2) + nv * sqrt(nl * nl * (1 - a2) + a2) + 1e-5);
    float3 f0 = lerp(0.04, c.rgb, metallic);
    float3 fresnel = f0 + (1 - f0) * pow(1 - saturate(dot(e, h)), 5);
    float3 diffuse = (1 - metallic) * c.rgb * (1 - fresnel);
    float3 specular = PI * distribution * visibility * fresnel;
    float3 color = (diffuse + specular) * nl * v.light_color.rgb + v.ambient.rgb * c.rgb * occlusion;
    color += m.emissive.xyz * sample(m.slots[4], i.uv).rgb;
    return float4(color, c.a);
}

[shader("fragment")]
float4 toon(Varyings i, bool front: SV_IsFrontFace) : SV_Target
{
    Material m = pc.materials[i.material];
    View v = *pc.view;
    float4 lit = base(m, i.uv);
    float3 shade = m.shade.xyz * sample(m.slots[2], i.uv).rgb;
    float3 n = normal(m, i, front);
    float3 e = normalize(v.eye.xyz - i.world);
    // MToon 1.0: shift the Lambert term, then a linear step whose width shrinks as toony goes to 1.
    float toony = m.toon.y;
    float s = dot(n, v.light_dir.xyz) + m.toon.x;
    s = saturate((s + 1 - toony) / max(2 - 2 * toony, 1e-4));
    float3 color = lerp(shade, lit.rgb, s) * v.light_color.rgb + lit.rgb * v.ambient.rgb;
    if (m.toon.z > 0)
        color += m.rim.xyz * pow(saturate(1 - dot(n, e)), m.toon.z);
    if (m.slots[5].tex.x != NONE)
    {
        // MToon's matcap frame: view x and y around the view direction, world up kept upright.
        float3 x = normalize(float3(e.z, 0, -e.x));
        float3 y = cross(e, x);
        float2 uv = float2(dot(x, n), -dot(y, n)) * 0.495 + 0.5;
        color += m.toon.w * sample(m.slots[5], float4(uv, uv)).rgb;
    }
    color += m.emissive.xyz * sample(m.slots[4], i.uv).rgb;
    return float4(color, lit.a);
}

[shader("fragment")]
float4 unlit(Varyings i) : SV_Target
{
    Material m = pc.materials[i.material];
    return base(m, i.uv);
}
