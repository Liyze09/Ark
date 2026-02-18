package io.github.liyze09.nexus.chunk;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout.OfByte;
import java.util.Collection;

import net.minecraft.core.SectionPos;

public class BuiltSection implements AutoCloseable {
    private final SectionPos sectionPos;
    private final Collection<SparseBlock> blocks;
    private final Arena arena = Arena.ofShared();
    private final MemorySegment dataMemorySegment;
    private final long header;

    public BuiltSection(
        SectionPos sectionPos,
        Collection<SparseBlock> blocks
    ) {
        this.sectionPos = sectionPos;
        this.blocks = blocks;
        this.dataMemorySegment = getBlockMemorySegment(this.arena, blocks);
        this.header = getHeader(sectionPos, blocks.size());
    }
    
    public SectionPos getSectionPos() {
        return sectionPos;
    }

    public Iterable<SparseBlock> getBlocks() {
        return blocks;
    }

    public MemorySegment getDataMemorySegment() {
        return dataMemorySegment;
    }

    public long getHeader() {
        return header;
    }

    @Override
    public void close() {
        arena.close();
    }

    private static MemorySegment getBlockMemorySegment(Arena arena, Collection<SparseBlock> blocks) {
        var segment = arena.allocate(blocks.size() * 5L);
        int i = 0;
        for (SparseBlock block : blocks) {
            var bytes = block.encodeAsBytes();
            segment.set(OfByte.JAVA_BYTE, i * 5L + 4, bytes[0]);
            segment.set(OfByte.JAVA_BYTE, i * 5L + 3, bytes[1]);
            segment.set(OfByte.JAVA_BYTE, i * 5L + 2, bytes[2]);
            segment.set(OfByte.JAVA_BYTE, i * 5L + 1, bytes[3]);
            segment.set(OfByte.JAVA_BYTE, i * 5L, bytes[4]);
            i++;
        }
        return segment;
    }

    private static long getHeader(SectionPos sectionPos, int blockCount) {
        return ((long)(sectionPos.x() & 0x3FFFFF)) |
               ((long)(sectionPos.z() & 0x3FFFFF) << 22) |
               ((long)(sectionPos.y() & 0xFF) << 44) |
               ((long)(blockCount & 0xFFF) << 52);
    }

}
