import common;

[vk::binding(0)]
StructuredBuffer<SectionHeader> SectionHeaders;
[vk::binding(1)]
StructuredBuffer<SparseBlock>    SparseBlocks;
[vk::binding(2)]
globallycoherent RWStructuredBuffer<uint> SectionCounter;
[vk::binding(3)]
globallycoherent RWStructuredBuffer<uint> MeshletCounter;
[vk::binding(4)]
globallycoherent RWStructuredBuffer<uint> VertexCounter;
[vk::binding(5)]
globallycoherent RWStructuredBuffer<uint> IndexCounter;
[vk::binding(6)]
globallycoherent RWStructuredBuffer<MeshletDesc> MeshletDescs;
[vk::binding(7)]
globallycoherent RWStructuredBuffer<Vertex> GlobalVertexBuffer;
[vk::binding(8)]
globallycoherent RWStructuredBuffer<uint> GlobalIndexBuffer;

static const uint MAX_VERTICES_PER_MESHLET = 64;
static const uint MAX_PRIMITIVES_PER_MESHLET = 126;
static const uint MAX_INDICES_PER_MESHLET = MAX_PRIMITIVES_PER_MESHLET * 3;

uint hash(float3 pos, uint materialID) {
    uint3 p = uint3(pos);
    p = p * 1664525u + 1013904223u;
    p.x += p.y * p.z + materialID;
    p.y += p.z * p.x;
    p.z += p.x * p.y;
    p = p ^ (p >> 16u);
    return p.x ^ p.y ^ p.z;
}

static const float3 faceVertices[6][4] = {
    { float3(0,1,0), float3(1,1,0), float3(1,1,1), float3(0,1,1) }, // UP
    { float3(0,0,0), float3(1,0,0), float3(1,0,1), float3(0,0,1) }, // DOWN
    { float3(0,0,0), float3(1,0,0), float3(1,1,0), float3(0,1,0) }, // NORTH
    { float3(0,0,1), float3(1,0,1), float3(1,1,1), float3(0,1,1) }, // SOUTH
    { float3(1,0,0), float3(1,0,1), float3(1,1,1), float3(1,1,0) }, // EAST
    { float3(0,0,0), float3(0,0,1), float3(0,1,1), float3(0,1,0) }  // WEST
};

bool tryReserveIndexSlots(inout uint indexCount, inout uint meshletFull, out uint startIdx) {
    uint old, out;
    do {
        old = indexCount;
        if (old + 3 > MAX_INDICES_PER_MESHLET) {
            InterlockedOr(meshletFull, 1);
            startIdx = 0;
            return false;
        }
        InterlockedCompareExchange(indexCount, old + 3, old, out);
    } while (out != old);
    startIdx = old;
    return true;
}

groupshared uint      bloomFilter_lo;
groupshared uint      bloomFilter_hi;
groupshared uint      vertexCount;
groupshared Vertex    vertices[MAX_VERTICES_PER_MESHLET];
groupshared uint      indexCount;
groupshared uint      indices[MAX_INDICES_PER_MESHLET];
groupshared uint      meshletFull;
[shader("task")]
[numthreads(32,1,1)]
void taskMain(uint gtid : SV_GroupThreadID, uint gid : SV_GroupID) {
    if (gtid == 0) {
        bloomFilter_lo = 0;
        bloomFilter_hi = 0;
        vertexCount = 0;
        indexCount = 0;
        meshletFull = 0;
    }
    GroupMemoryBarrierWithGroupSync();

    uint headerCount;
    SectionHeaders.GetDimensions(1, headerCount);

    while (true) {
        uint sectionIdx;
        if (gtid == 0) {
            uint original;
            InterlockedAdd(SectionCounter[0], -1, original);
            if (original == 0) {
                sectionIdx = 0xFFFFFFFF;
            } else {
                sectionIdx = original - 1;
            }
        }
        sectionIdx = WaveReadLaneFirst(sectionIdx);
        if (sectionIdx >= headerCount) break;

        SectionHeader header = SectionHeaders[sectionIdx];
        uint blockCount = header.blockCount;
        uint offset = header.offset;
        int3 sectionBase = int3(header.x, header.y, header.z) * 16;

        for (uint i = gtid; i < blockCount; i += 32) {
            SparseBlock block = SparseBlocks[offset + i];
            uint8_t mask = block.faceMask;
            int3 localPos = int3(block.localX, block.localY, block.localZ);
            int3 worldBase = sectionBase + localPos;
            uint materialID = uint(block.blockState);

            [unroll]
            for (int f = 0; f < 6; ++f) {
                if (mask & (1u << f)) {
                    uint idx[4];
                    for (int v = 0; v < 4; ++v) {
                        float3 pos = worldBase + faceVertices[f][v];
                        uint hashVal = hash(pos, materialID);
                        uint bit1 = hashVal & 0x3F;
                        uint bit2 = (hashVal >> 6) & 0x3F;
                        uint64_t bloomMask = (1ull << bit1) | (1ull << bit2);
                        uint mask_lo = uint(bloomMask & 0xFFFFFFFF);
                        uint mask_hi = uint(bloomMask >> 32);

                        uint filter_lo = bloomFilter_lo;
                        uint filter_hi = bloomFilter_hi;
                        bool maybeExists = ((filter_lo & mask_lo) == mask_lo) &&
                                           ((filter_hi & mask_hi) == mask_hi);

                        uint vertexIdx = 0xFFFFFFFF;
                        if (maybeExists) {
                            for (uint j = 0; j < vertexCount; ++j) {
                                if (all(vertices[j].position == pos) && vertices[j].materialID == materialID) {
                                    vertexIdx = j;
                                    break;
                                }
                            }
                        }

                        if (vertexIdx == 0xFFFFFFFF) {
                            if (meshletFull != 0) {
                                vertexIdx = 0xFFFFFFFF;
                            } else {
                                uint oldVCount;
                                InterlockedAdd(vertexCount, 1, oldVCount);
                                if (oldVCount < MAX_VERTICES_PER_MESHLET) {
                                    vertexIdx = oldVCount;
                                    vertices[vertexIdx].position = pos;
                                    vertices[vertexIdx].materialID = materialID;
                                    InterlockedOr(bloomFilter_lo, mask_lo);
                                    InterlockedOr(bloomFilter_hi, mask_hi);
                                } else {
                                    InterlockedOr(meshletFull, 1);
                                    vertexIdx = 0xFFFFFFFF;
                                }
                            }
                        }
                        idx[v] = vertexIdx;
                    }

                    if (meshletFull == 0) {
                        bool allValid = true;
                        for (int v = 0; v < 4; ++v) {
                            if (idx[v] >= MAX_VERTICES_PER_MESHLET) { allValid = false; break; }
                        }
                        if (allValid) {
                            uint start;
                            if (tryReserveIndexSlots(indexCount, meshletFull, start)) {
                                indices[start]   = idx[0];
                                indices[start+1] = idx[1];
                                indices[start+2] = idx[2];
                                if (tryReserveIndexSlots(indexCount, meshletFull, start)) {
                                    indices[start]   = idx[0];
                                    indices[start+1] = idx[2];
                                    indices[start+2] = idx[3];
                                }
                            }
                        }
                    }

                    if (meshletFull != 0) {
                        GroupMemoryBarrierWithGroupSync();
                        if (gtid == 0) {

                            uint meshletID, vertOffset, idxOffset;
                            InterlockedAdd(MeshletCounter[0], 1, meshletID);
                            InterlockedAdd(VertexCounter[0], vertexCount, vertOffset);
                            InterlockedAdd(IndexCounter[0], indexCount, idxOffset);

                            MeshletDesc desc;
                            desc.vertexOffset = vertOffset;
                            desc.indexOffset = idxOffset;
                            desc.vertexCount = vertexCount;
                            desc.primitiveCount = indexCount / 3;
                            MeshletDescs[meshletID] = desc;

                            for (uint v = 0; v < vertexCount; ++v)
                                GlobalVertexBuffer[vertOffset + v] = vertices[v];
                            for (uint ix = 0; ix < indexCount; ++ix)
                                GlobalIndexBuffer[idxOffset + ix] = indices[ix];

                            bloomFilter_lo = 0;
                            bloomFilter_hi = 0;
                            vertexCount = 0;
                            indexCount = 0;
                            meshletFull = 0;
                        }
                        GroupMemoryBarrierWithGroupSync();
                    }
                }
            }
        }

        GroupMemoryBarrierWithGroupSync();
    }


    if (gtid == 0 && vertexCount > 0) {
        uint meshletID, vertOffset, idxOffset;
        InterlockedAdd(MeshletCounter[0], 1, meshletID);
        InterlockedAdd(VertexCounter[0], vertexCount, vertOffset);
        InterlockedAdd(IndexCounter[0], indexCount, idxOffset);

        MeshletDesc desc;
        desc.vertexOffset = vertOffset;
        desc.indexOffset = idxOffset;
        desc.vertexCount = vertexCount;
        desc.primitiveCount = indexCount / 3;
        MeshletDescs[meshletID] = desc;

        for (uint v = 0; v < vertexCount; ++v)
            GlobalVertexBuffer[vertOffset + v] = vertices[v];
        for (uint ix = 0; ix < indexCount; ++ix)
            GlobalIndexBuffer[idxOffset + ix] = indices[ix];
    }
}

// -------------------- Mesh Shader --------------------
/*
[shader("mesh")]
[numthreads(32,1,1)]
[outputtopology("triangle")]
void meshMain(uint gtid : SV_GroupThreadID,
              uint gid : SV_GroupID,
              uint meshletID : SV_DispatchThreadID)
{
    MeshletDesc desc = MeshletDescs[meshletID];
    SetMeshOutputCounts(desc.vertexCount, desc.primitiveCount, 0);

    if (gtid < desc.vertexCount) {
        Vertex v = GlobalVertexBuffer[desc.vertexOffset + gtid];
        SetMeshOutput(gtid, v);
    }

    for (uint i = gtid; i < desc.primitiveCount; i += 32) {
        uint idx0 = GlobalIndexBuffer[desc.indexOffset + i*3];
        uint idx1 = GlobalIndexBuffer[desc.indexOffset + i*3 + 1];
        uint idx2 = GlobalIndexBuffer[desc.indexOffset + i*3 + 2];
        SetMeshOutput(i, uint3(idx0, idx1, idx2)); 
    }
}*/