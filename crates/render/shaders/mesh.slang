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
    float4 shade;    // xyz, toon shadow colour; w 1: light plus ambient, clamped, lights the blend (ToonLight)
    float4 rim;      // xyz, toon rim colour; w the matcap blend: 0 add, 1 multiply, 2 replace (MatcapBlend)
    float4 params;   // metallic, roughness, normal scale, alpha cutoff (0: no cutoff)
    float4 toon;     // shade shift, shade toony, rim power, matcap strength
    Slot slots[6];   // base, normal, metallic-roughness or shade, occlusion, emissive, matcap
    float4 outline;  // toon outline colour in xyz, width in metres in w
    Slot outline_width; // multiplies the width by its G channel
    float4 rim_shade;      // toon rim shade colour in xyz, strength in w (0: off)
    float4 rim_shade_ramp; // its ramp's start and end, fresnel power
    float4 matcap;         // toon matcap tint in xyz, its emission weight in w
    Slot matcap_mask;      // scales the matcap strength by its R channel
    float4 light;          // the clamped toon light's lower and upper bound
    float4 shade_mix;      // shadow strength, main strength, 1: the shadow stays below the lit colour
    float4 shade_second;   // second shadow colour in xyz, strength in w
    float4 shade_ramps;    // the second shadow's ramp start and end, the border colour's (half-Lambert)
    float4 shade_border;   // shadow border colour in xyz; the emission's main strength in w
    float4 rim_ramp;       // rim ramp start and end, shadow mask, main strength
    float4 tone;           // the base texture's hue shift, saturation, value, gamma (0, 1, 1, 1: none)
    Slot outline_tex;      // multiplies the outline colour (lilToon's _OutlineTex)
    float4 matcap2;        // the second toon matcap's tint in xyz, strength in w
    float4 matcap2_mode;   // its blend (as rim.w) and emission weight
    Slot matcap2_tex;
    Slot matcap2_mask;
};

// A linear ramp from `lo` to `hi`, clamped to 0 to 1 (lilToon's tooning scale without anti-aliasing).
float ramp(float x, float lo, float hi)
{
    return saturate((x - lo) / max(hi - lo, 1e-4));
}

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
    View* view; // one record per view of a multiview pass (ADR 0120)
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
    nointerpolation uint view : VIEW;
};

// A skinned vertex of draw `d` at world position `p`, seen from view `view`.
Varyings place(Draw d, Skinned s, float4 uv, float3 p, uint view)
{
    View v = pc.view[view];
    Varyings o;
    o.position = v.view_proj[0] * p.x + v.view_proj[1] * p.y + v.view_proj[2] * p.z + v.view_proj[3];
    o.world = p;
    o.normal = s.normal.xyz;
    o.tangent = s.tangent;
    o.uv = uv;
    o.material = d.material;
    o.view = view;
    return o;
}

[shader("vertex")]
Varyings vertex(uint index: SV_VulkanVertexID, uint instance: SV_VulkanInstanceID, uint view: SV_ViewID)
{
    Draw d = pc.draws[instance];
    Skinned s = pc.skinned[d.skinned + index];
    return place(d, s, pc.attrs[d.attrs + index], s.position.xyz, view);
}

// Toon outlines (ADR 0034): the vertex pushed out along its skinned normal by the outline width in metres, times
// the width texture's G channel (MToon 1.0) sampled at the top mip.
[shader("vertex")]
Varyings outline_vertex(uint index: SV_VulkanVertexID, uint instance: SV_VulkanInstanceID, uint view: SV_ViewID)
{
    Draw d = pc.draws[instance];
    Skinned s = pc.skinned[d.skinned + index];
    Material m = pc.materials[d.material];
    float4 uv = pc.attrs[d.attrs + index];
    float width = m.outline.w;
    Slot w = m.outline_width;
    if (w.tex.x != NONE)
    {
        float2 c = (w.tex.y == 0 ? uv.xy : uv.zw) * w.st.xy + w.st.zw;
        width *= textures[NonUniformResourceIndex(w.tex.x)].SampleLevel(c, 0).g;
    }
    float3 n = s.normal.xyz;
    float l = length(n);
    float3 p = s.position.xyz + (l > 0 ? n * (width / l) : 0);
    return place(d, s, uv, p, view);
}

// The slot's texel, or white without a texture.
float4 sample(Slot s, float4 uv)
{
    if (s.tex.x == NONE)
        return 1;
    float2 c = (s.tex.y == 0 ? uv.xy : uv.zw) * s.st.xy + s.st.zw;
    return textures[NonUniformResourceIndex(s.tex.x)].Sample(c);
}

// lilToon's tone correction (`lilToneCorrection`): gamma, then hue shift, saturation and value scaled in HSV.
float3 toned(Material m, float3 c)
{
    float4 hsvg = m.tone;
    if (all(hsvg == float4(0, 1, 1, 1)))
        return c;
    c = pow(abs(c), hsvg.w);
    float4 p = (c.b > c.g) ? float4(c.bg, -1.0, 2.0 / 3.0) : float4(c.gb, 0.0, -1.0 / 3.0);
    float4 q = (p.x > c.r) ? float4(p.xyw, c.r) : float4(c.r, p.yzx);
    float d = q.x - min(q.w, q.y);
    float e = 1.0e-10;
    float3 hsv = float3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
    hsv = float3(hsv.x + hsvg.x, saturate(hsv.y * hsvg.y), saturate(hsv.z * hsvg.z));
    return hsv.z - hsv.z * hsv.y + hsv.z * hsv.y * saturate(abs(frac(hsv.x + float3(1.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0) - 1.0);
}

// Base colour times its (tone-corrected) texture; discards below the alpha cutoff.
float4 base(Material m, float4 uv)
{
    float4 t = sample(m.slots[0], uv);
    t.rgb = toned(m, t.rgb);
    float4 c = m.base_color * t;
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
    View v = pc.view[i.view];
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

// `color` with a matcap (`tex` at matcap coordinates `uv`, tinted) blended in by `strength` times the mask's red
// channel: 0 adds, 1 multiplies, 2 replaces the colour by the matcap lit by `light`; then `emission` times the masked
// matcap adds (Poiyomi's matcap emission). Without a texture, the colour as it is.
float3 matcap(float3 color, float3 light, float2 uv, float4 uvs, Slot tex, float3 tint, float strength, Slot mask,
    float blend, float emission)
{
    if (tex.tex.x == NONE)
        return color;
    float3 mc = sample(tex, float4(uv, uv)).rgb * tint;
    float w = sample(mask, uvs).r;
    if (blend > 1.5)
        color = lerp(color, mc * light, strength * w); // Poiyomi replace (of the colour before lighting)
    else if (blend > 0.5)
        color = lerp(color, color * mc, strength * w); // VRChat MatCap Lit, lilToon multiply
    else
        color += strength * w * mc;
    return color + emission * w * mc;
}

[shader("fragment")]
float4 toon(Varyings i, bool front: SV_IsFrontFace) : SV_Target
{
    Material m = pc.materials[i.material];
    View v = pc.view[i.view];
    float4 lit = base(m, i.uv);
    float3 shade_tex = sample(m.slots[2], i.uv).rgb;
    if (m.slots[2].tex.x == m.slots[0].tex.x)
        shade_tex = toned(m, shade_tex); // the shadow takes the main texture as lilToon corrects it
    float3 shade = m.shade.xyz * shade_tex;
    float3 n = normal(m, i, front);
    float3 e = normalize(v.eye.xyz - i.world);
    // lilToon's shade bands (neutral for the other families): a second shadow below its ramp, the albedo
    // multiplied in, the border colour towards the lit colour, then the strength.
    float ln = dot(n, v.light_dir.xyz) * 0.5 + 0.5;
    shade = lerp(shade, lit.rgb * m.shade_second.xyz, m.shade_second.w * (1 - ramp(ln, m.shade_ramps.x, m.shade_ramps.y)));
    shade = lerp(shade, shade * lit.rgb, m.shade_mix.y);
    if (m.shade_mix.z > 0.5)
        shade = min(shade, lit.rgb);
    shade = lerp(shade, lit.rgb, ramp(ln, m.shade_ramps.z, m.shade_ramps.w) * m.shade_border.xyz);
    shade = lerp(lit.rgb, shade, m.shade_mix.x);
    // MToon 1.0: shift the Lambert term, then a linear step whose width shrinks as toony goes to 1.
    float toony = m.toon.y;
    float s = dot(n, v.light_dir.xyz) + m.toon.x;
    s = saturate((s + 1 - toony) / max(2 - 2 * toony, 1e-4));
    float3 color = lerp(shade, lit.rgb, s);
    float3 light = v.light_color.rgb + v.ambient.rgb; // what a lit surface's colour is multiplied by
    if (m.shade.w > 0.5)
    {
        light = clamp(light, m.light.x, m.light.y);
        color *= light; // lilToon, Poiyomi, UTS2
    }
    else
        color = color * v.light_color.rgb + lit.rgb * v.ambient.rgb; // MToon
    if (m.rim_shade.w > 0)
    {
        // lilToon's rim shade, after lighting.
        float f = pow(saturate(1 - abs(dot(n, e))), m.rim_shade_ramp.z);
        float r = saturate((f - m.rim_shade_ramp.x) / max(m.rim_shade_ramp.y - m.rim_shade_ramp.x, 1e-4));
        color = lerp(color, color * m.rim_shade.xyz, r * m.rim_shade.w);
    }
    if (m.toon.z > 0)
    {
        // The fresnel rim; lilToon ramps it, fades it in shadow and tints it by the albedo.
        float f = ramp(pow(saturate(1 - dot(n, e)), m.toon.z), m.rim_ramp.x, m.rim_ramp.y);
        f = lerp(f, f * s, m.rim_ramp.z);
        color += m.rim.xyz * lerp(1, lit.rgb, m.rim_ramp.w) * f;
    }
    if (m.slots[5].tex.x != NONE || m.matcap2_tex.tex.x != NONE)
    {
        // MToon's matcap frame: view x and y around the view direction, world up kept upright.
        float3 x = normalize(float3(e.z, 0, -e.x));
        float3 y = cross(e, x);
        float2 uv = float2(dot(x, n), -dot(y, n)) * 0.495 + 0.5;
        color = matcap(color, light, uv, i.uv, m.slots[5], m.matcap.xyz, m.toon.w, m.matcap_mask, m.rim.w, m.matcap.w);
        color = matcap(color, light, uv, i.uv, m.matcap2_tex, m.matcap2.xyz, m.matcap2.w, m.matcap2_mask,
            m.matcap2_mode.x, m.matcap2_mode.y);
    }
    color += m.emissive.xyz * sample(m.slots[4], i.uv).rgb * lerp(1, lit.rgb, m.shade_border.w);
    return float4(color, lit.a);
}

[shader("fragment")]
float4 unlit(Varyings i) : SV_Target
{
    Material m = pc.materials[i.material];
    return base(m, i.uv);
}

// The outline colour times its texture, unlit, with the base colour's alpha and cutoff.
[shader("fragment")]
float4 outline(Varyings i) : SV_Target
{
    Material m = pc.materials[i.material];
    return float4(m.outline.xyz * sample(m.outline_tex, i.uv).rgb, base(m, i.uv).a);
}
