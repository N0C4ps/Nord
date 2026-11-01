Texture2D<float> sourceDepth : register(t0);
RWTexture2D<float> destinationDepth : register(u0);

[numthreads(8, 8, 1)]
void CSBuild(uint3 dispatchThreadId : SV_DispatchThreadID)
{
    uint sourceWidth;
    uint sourceHeight;
    uint destinationWidth;
    uint destinationHeight;
    sourceDepth.GetDimensions(sourceWidth, sourceHeight);
    destinationDepth.GetDimensions(destinationWidth, destinationHeight);

    const uint2 destination = dispatchThreadId.xy;
    if (destination.x >= destinationWidth || destination.y >= destinationHeight)
    {
        return;
    }

    const uint2 firstSource = destination * 4;
    float farthestDepth = 0.0;
    [unroll]
    for (uint y = 0; y < 4; ++y)
    {
        [unroll]
        for (uint x = 0; x < 4; ++x)
        {
            const uint2 source = firstSource + uint2(x, y);
            if (source.x < sourceWidth && source.y < sourceHeight)
            {
                farthestDepth = max(farthestDepth, sourceDepth.Load(int3(source, 0)));
            }
        }
    }
    destinationDepth[destination] = farthestDepth;
}
