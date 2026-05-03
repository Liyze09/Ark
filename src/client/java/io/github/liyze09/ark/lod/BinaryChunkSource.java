package io.github.liyze09.ark.lod;

import net.minecraft.world.level.ChunkPos;

import java.lang.foreign.MemorySegment;
import java.util.concurrent.CompletableFuture;

public interface BinaryChunkSource {
    CompletableFuture<MemorySegment> getBinaryChunk(ChunkPos pos);
}
