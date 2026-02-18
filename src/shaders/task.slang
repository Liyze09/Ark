import common;

[[vk::binding(0)]]
StructuredBuffer<SectionHeader, ScalarDataLayout> sectionHeaders;
[[vk::binding(1)]]
StructuredBuffer<SparseBlock, ScalarDataLayout> sectionData;

[shader("task")]
[outputtopology("triangle")]
[numthreads(32, 1, 1)]
void main(
    uint3 id : SV_DispatchThreadID
) {
    int sectionIndex = id.x;
    if (sectionHeaders.getCount() <= sectionIndex) {
        return;
    }
    SectionHeader sectionHeader = sectionHeaders[sectionIndex];
    for (int i = 0; i < sectionHeader.blockCount; i++) {
        SparseBlock block = sectionData[sectionHeader.offset + i];
        
    }
}