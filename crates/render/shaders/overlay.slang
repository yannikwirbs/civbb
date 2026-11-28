// Screen-space UI (ADR 0117): triangles in window pixels pulled by address, in plain colour or times a texture of
// the bindless array. Compiled by `cargo xtask shaders`.

static const uint NONE = 0xffffffff;

// `Vertex` in `overlay.rs`: 48 bytes.
struct Vertex
{
    float2 pos;   // window pixels, origin top left
    float2 uv;
    float4 color; // linear, straight alpha
    uint texture; // bindless index or NONE
    uint pad0;
    uint pad1;
    uint pad2;
};

struct Push
{
    Vertex* vertices;
    float2 scale; // 2 / extent
};

[[vk::push_constant]] Push pc;

[[vk::binding(0, 0)]] Sampler2D textures[];

struct Varyings
{
    float4 position : SV_Position;
    float2 uv : TEXCOORD;
    float4 color : COLOR;
    nointerpolation uint texture : TEXTURE;
};

[shader("vertex")]
Varyings vertex(uint index: SV_VulkanVertexID)
{
    Vertex v = pc.vertices[index];
    Varyings o;
    o.position = float4(v.pos * pc.scale - 1.0, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.texture = v.texture;
    return o;
}

[shader("fragment")]
float4 fragment(Varyings i) : SV_Target
{
    float4 c = i.color;
    if (i.texture != NONE)
        c *= textures[NonUniformResourceIndex(i.texture)].Sample(i.uv);
    return c;
}
