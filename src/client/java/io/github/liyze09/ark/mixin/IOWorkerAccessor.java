package io.github.liyze09.ark.mixin;

import net.minecraft.util.thread.PriorityConsecutiveExecutor;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.IOWorker;
import net.minecraft.world.level.chunk.storage.RegionFileStorage;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

import java.util.SequencedMap;
import java.util.concurrent.atomic.AtomicBoolean;

@Mixin(IOWorker.class)
public interface IOWorkerAccessor {
    @Accessor("storage")
    RegionFileStorage getRegionFileStorage();

    @Accessor("consecutiveExecutor")
    PriorityConsecutiveExecutor getConsecutiveExecutor();

    @Accessor("shutdownRequested")
    AtomicBoolean isShutdownRequested();

    @Accessor("pendingWrites")
    SequencedMap<ChunkPos, IOWorker.PendingStore> getPendingWrites();

    @Invoker("tellStorePending")
    void tellStorePending();
}
