cbuffer CullingData : register(b0)
{
    float4 frustumPlanes[6];
    uint sourceInstanceCount;
    uint meshIndexCount;
    float boundsRadius;
    uint _padding;
};

RWByteAddressBuffer outputBuffer : register(u0);

[numthreads(1, 1, 1)]
void CSInitialize(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    outputBuffer.Store(0, meshIndexCount);
    outputBuffer.Store(4, 0);
    outputBuffer.Store(8, 0);
    outputBuffer.Store(12, 0);
    outputBuffer.Store(16, 0);
}

ByteAddressBuffer sourceInstances : register(t0);
RWByteAddressBuffer cullArguments : register(u1);

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

    uint outputIndex;
    cullArguments.InterlockedAdd(4, 1, outputIndex);
    const uint outputByteOffset = outputIndex * 24;
    outputBuffer.Store3(outputByteOffset, sourceInstances.Load3(sourceByteOffset));
    outputBuffer.Store3(outputByteOffset + 12, sourceInstances.Load3(sourceByteOffset + 12));
}
