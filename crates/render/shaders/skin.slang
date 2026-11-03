// Compute skinning and sparse blendshapes (design doc §6, ADR 0022). Compiled by `cargo xtask shaders`.
//
// `morph` adds the deltas of the shapes with non-zero weight into a per-vertex fixed-point accumulator with integer
// atomics (one thread per active delta; order-independent, so deterministic). `skin` adds the accumulator to the
// rest vertex and applies linear blend skinning with four influences.

struct Vertex
{
    float4 position;
    float4 normal;
    float4 tangent; // w: handedness
    uint4 joints;
    float4 weights;
};

struct Delta
{
    float4 position; // w: the vertex index, as bits
    float4 normal;
};

// One shape with non-zero weight: its deltas `first .. first + count`, and its threads `start .. start + count`.
struct Active
{
    uint first;
    uint count;
    uint start;
    float weight;
};

struct Skinned
{
    float4 position;
    float4 normal;
    float4 tangent;
};

struct Push
{
    Vertex* vertices;
    Delta* deltas;
    Active* active;
    float4* joints; // four columns per joint
    int* accum;     // six per vertex: position xyz, normal xyz
    Skinned* out;
    uint vertex_count;
    uint active_count;
    uint thread_count; // active deltas in total
    float scale;       // fixed-point units per metre
};

[[vk::push_constant]] Push pc;

[shader("compute")]
[numthreads(64, 1, 1)]
void morph(uint3 id: SV_DispatchThreadID)
{
    uint t = id.x;
    if (t >= pc.thread_count)
        return;
    // The last active shape starting at or before t.
    uint lo = 0;
    uint hi = pc.active_count;
    while (hi - lo > 1)
    {
        uint mid = (lo + hi) / 2;
        if (pc.active[mid].start <= t)
            lo = mid;
        else
            hi = mid;
    }
    Active a = pc.active[lo];
    Delta d = pc.deltas[a.first + t - a.start];
    uint v = asuint(d.position.w);
    int3 p = int3(round(d.position.xyz * a.weight * pc.scale));
    int3 n = int3(round(d.normal.xyz * a.weight * pc.scale));
    InterlockedAdd(pc.accum[v * 6 + 0], p.x);
    InterlockedAdd(pc.accum[v * 6 + 1], p.y);
    InterlockedAdd(pc.accum[v * 6 + 2], p.z);
    InterlockedAdd(pc.accum[v * 6 + 3], n.x);
    InterlockedAdd(pc.accum[v * 6 + 4], n.y);
    InterlockedAdd(pc.accum[v * 6 + 5], n.z);
}

float3 transform(uint j, float3 v, float w)
{
    return pc.joints[j * 4].xyz * v.x + pc.joints[j * 4 + 1].xyz * v.y + pc.joints[j * 4 + 2].xyz * v.z +
           pc.joints[j * 4 + 3].xyz * w;
}

float3 unit_or_zero(float3 v)
{
    float l = length(v);
    return l > 0 ? v / l : v;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void skin(uint3 id: SV_DispatchThreadID)
{
    uint v = id.x;
    if (v >= pc.vertex_count)
        return;
    Vertex x = pc.vertices[v];
    float3 p = x.position.xyz;
    float3 n = x.normal.xyz;
    if (pc.active_count > 0)
    {
        int b = v * 6;
        p += float3(pc.accum[b], pc.accum[b + 1], pc.accum[b + 2]) / pc.scale;
        n += float3(pc.accum[b + 3], pc.accum[b + 4], pc.accum[b + 5]) / pc.scale;
    }
    float3 sp = 0;
    float3 sn = 0;
    float3 st = 0;
    for (uint i = 0; i < 4; i++)
    {
        float w = x.weights[i];
        if (w == 0)
            continue;
        uint j = x.joints[i];
        sp += w * transform(j, p, 1);
        sn += w * transform(j, n, 0);
        st += w * transform(j, x.tangent.xyz, 0);
    }
    pc.out[v].position = float4(sp, 1);
    pc.out[v].normal = float4(unit_or_zero(sn), 0);
    pc.out[v].tangent = float4(unit_or_zero(st), x.tangent.w);
}
