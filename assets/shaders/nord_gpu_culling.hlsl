cbuffer CullingData : register(b0)
{
    float4 frustumPlanes[6];
    uint sourceInstanceCount;
    uint meshIndexCount;
    float boundsRadius;
    uint hizMipCount;
    column_major float4x4 viewProjection;
    float3 boundsHalfExtents;
    uint _padding;
};

RWByteAddressBuffer outputBuffer : register(u0);
RWByteAddressBuffer cullArguments : register(u1);
RWByteAddressBuffer cullStatistics : register(u2);

[numthreads(1, 1, 1)]
void CSInitialize(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    cullArguments.Store(0, meshIndexCount);
    cullArguments.Store(4, 0);
    cullArguments.Store(8, 0);
    cullArguments.Store(12, 0);
    cullArguments.Store(16, 0);
    cullStatistics.Store(0, 0);
    cullStatistics.Store(4, 0);
}

ByteAddressBuffer sourceInstances : register(t0);
Texture2D<float> hiZDepths[15] : register(t1);

[numthreads(64, 1, 1)]
void CSCull(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    const uint sourceIndex = dispatchThreadId.x;
    if (sourceIndex >= sourceInstanceCount)
    {
        return;
    }

    const uint sourceByteOffset = sourceIndex * 24;
    const float3 center = asfloat(sourceInstances.Load3(sourceByteOffset));
    [unroll]
    for (uint planeIndex = 0; planeIndex < 6; ++planeIndex)
    {
        const float4 plane = frustumPlanes[planeIndex];
        if (dot(plane.xyz, center) + plane.w < -boundsRadius)
        {
            return;
        }
    }

    uint ignored;
    cullStatistics.InterlockedAdd(0, 1, ignored);
    uint outputIndex;
    cullArguments.InterlockedAdd(4, 1, outputIndex);
    const uint outputByteOffset = outputIndex * 24;
    outputBuffer.Store3(outputByteOffset, sourceInstances.Load3(sourceByteOffset));
    outputBuffer.Store3(outputByteOffset + 12, sourceInstances.Load3(sourceByteOffset + 12));
}

[numthreads(64, 1, 1)]
void CSCullHiZ(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    const uint sourceIndex = dispatchThreadId.x;
    if (sourceIndex >= sourceInstanceCount)
    {
        return;
    }

    const uint sourceByteOffset = sourceIndex * 24;
    const float3 center = asfloat(sourceInstances.Load3(sourceByteOffset));
    [unroll]
    for (uint planeIndex = 0; planeIndex < 6; ++planeIndex)
    {
        const float4 plane = frustumPlanes[planeIndex];
        if (dot(plane.xyz, center) + plane.w < -boundsRadius)
        {
            return;
        }
    }

    uint ignored;
    cullStatistics.InterlockedAdd(0, 1, ignored);

    float2 minimumUv = float2(1.0, 1.0);
    float2 maximumUv = float2(0.0, 0.0);
    float nearestDepth = 1.0;
    [unroll]
    for (uint corner = 0; corner < 8; ++corner)
    {
        const float3 sign = float3(
            (corner & 1) == 0 ? -1.0 : 1.0,
            (corner & 2) == 0 ? -1.0 : 1.0,
            (corner & 4) == 0 ? -1.0 : 1.0);
        const float3 worldPosition = center + sign * boundsHalfExtents;
        const float4 clip = mul(viewProjection, float4(worldPosition, 1.0));
        if (clip.w <= 1.0e-5)
        {
            uint visibleIndex;
            cullArguments.InterlockedAdd(4, 1, visibleIndex);
            const uint visibleByteOffset = visibleIndex * 24;
            outputBuffer.Store3(visibleByteOffset, sourceInstances.Load3(sourceByteOffset));
            outputBuffer.Store3(visibleByteOffset + 12, sourceInstances.Load3(sourceByteOffset + 12));
            return;
        }
        const float3 ndc = clip.xyz / clip.w;
        const float2 uv = float2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
        minimumUv = min(minimumUv, uv);
        maximumUv = max(maximumUv, uv);
        nearestDepth = min(nearestDepth, ndc.z);
    }

    minimumUv = saturate(minimumUv);
    maximumUv = saturate(maximumUv);
    uint selectedMip = 0;
    uint2 firstTexel = 0;
    uint2 lastTexel = 0;
    bool coveredByFourTexels = false;
    [loop]
    for (uint mip = 0; mip < hizMipCount; ++mip)
    {
        uint width;
        uint height;
        hiZDepths[NonUniformResourceIndex(mip)].GetDimensions(width, height);
        const uint2 extent = uint2(width, height);
        firstTexel = min(uint2(floor(minimumUv * extent)), extent - 1);
        lastTexel = min(uint2(floor(maximumUv * extent)), extent - 1);
        if (all(lastTexel - firstTexel <= 1))
        {
            selectedMip = mip;
            coveredByFourTexels = true;
            break;
        }
    }

    bool occluded = false;
    if (coveredByFourTexels && nearestDepth >= 0.0 && nearestDepth <= 1.0)
    {
        float farthestOccluderDepth = 0.0;
        [unroll]
        for (uint y = 0; y < 2; ++y)
        {
            [unroll]
            for (uint x = 0; x < 2; ++x)
            {
                const uint2 texel = firstTexel + uint2(x, y);
                if (all(texel <= lastTexel))
                {
                    farthestOccluderDepth = max(
                        farthestOccluderDepth,
                        hiZDepths[NonUniformResourceIndex(selectedMip)].Load(int3(texel, 0)));
                }
            }
        }
        occluded = nearestDepth > farthestOccluderDepth + 0.001;
    }

    if (occluded)
    {
        cullStatistics.InterlockedAdd(4, 1, ignored);
        return;
    }

    uint outputIndex;
    cullArguments.InterlockedAdd(4, 1, outputIndex);
    const uint outputByteOffset = outputIndex * 24;
    outputBuffer.Store3(outputByteOffset, sourceInstances.Load3(sourceByteOffset));
    outputBuffer.Store3(outputByteOffset + 12, sourceInstances.Load3(sourceByteOffset + 12));
}
