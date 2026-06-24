package io.github.liyze09.ark.lod;

import io.github.liyze09.ark.mixin.IOWorkerAccessor;
import io.github.liyze09.ark.mixin.SimpleRegionStorageAccessor;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.RegionFile;
import org.apache.commons.io.output.TeeOutputStream;
import org.jspecify.annotations.NonNull;

import java.io.*;
import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.util.Arrays;
import java.util.Objects;
import java.util.concurrent.CompletableFuture;

public class ServerLevelBinaryChunkSource implements BinaryChunkSource {
    private final IOWorkerAccessor worker;

    public ServerLevelBinaryChunkSource(@NonNull ServerLevel level) {
        this.worker = (IOWorkerAccessor) ((SimpleRegionStorageAccessor) level.getChunkSource().chunkMap).getIOWorker();
    }

    @Override
    public CompletableFuture<MemorySegment> getBinaryChunk(ChunkPos pos, Arena arena) {
        return submitThrowingTask(() -> {
            var pendingStore = worker.getPendingWrites().get(pos);
            if (pendingStore != null) {
                CompoundTag data = pendingStore.data;
                if (data == null) {
                    return null;
                }

                var buf = new ByteArrayOutputStream(8192);
                RegionFile regionFile = worker.getRegionFileStorage().getRegionFile(pos);
                DataOutputStream regionOut = regionFile.getChunkDataOutputStream(pos);
                try (TeeOutputStream tee = new TeeOutputStream(regionOut, buf);
                     DataOutputStream dataOut = new DataOutputStream(tee)) {
                    NbtIo.write(data, dataOut);
                }
                var segment = arena.allocate(buf.size());
                MemorySegment.copy(
                        buf.getBackedByteArray(),
                        0,
                        segment,
                        ValueLayout.JAVA_BYTE,
                        0,
                        buf.size()
                );
                return segment;
            }

            RegionFile regionFile = worker.getRegionFileStorage().getRegionFile(pos);
            DataInputStream chunkStream = regionFile.getChunkDataInputStream(pos);
            if (chunkStream == null) {
                return null;
            }
            var buf = new ByteArrayOutputStream(8192);
            try (chunkStream) {
                chunkStream.transferTo(buf);
            }
            var segment = arena.allocate(buf.size());
            MemorySegment.copy(
                    buf.getBackedByteArray(),
                    0,
                    segment,
                    ValueLayout.JAVA_BYTE,
                    0,
                    buf.size()
            );
            return segment;
        });
    }

    private <T> @NonNull CompletableFuture<T> submitThrowingTask(final ThrowingSupplier<T> task) {
        return worker.getConsecutiveExecutor().scheduleWithResult(Priority.FOREGROUND.ordinal(), future -> {
            if (!worker.isShutdownRequested().get()) {
                try {
                    future.complete(task.get());
                } catch (Exception e) {
                    future.completeExceptionally(e);
                }
            }

            worker.tellStorePending();
        });
    }

    private enum Priority {
        FOREGROUND,
        BACKGROUND,
        SHUTDOWN
    }

    @FunctionalInterface
    private interface ThrowingSupplier<T> {
        T get() throws Exception;
    }

    private static final class ByteArrayOutputStream extends OutputStream {
        private byte[] buf;
        private int count;

        public ByteArrayOutputStream(int size) {
            if (size < 0) {
                throw new IllegalArgumentException("Negative initial size: " + size);
            } else {
                this.buf = new byte[size];
            }
        }

        private void ensureCapacity(int minCapacity) {
            int oldCapacity = this.buf.length;
            int minGrowth = minCapacity - oldCapacity;
            if (minGrowth > 0) {
                int prefLength = oldCapacity + Math.max(minGrowth, oldCapacity);
                this.buf = Arrays.copyOf(this.buf, prefLength);
            }

        }

        @Override
        public synchronized void write(int b) {
            this.ensureCapacity(this.count + 1);
            this.buf[this.count] = (byte)b;
            ++this.count;
        }

        @Override
        public synchronized void write(byte @NonNull [] b, int off, int len) {
            Objects.checkFromIndexSize(off, len, b.length);
            this.ensureCapacity(this.count + len);
            System.arraycopy(b, off, this.buf, this.count, len);
            this.count += len;
        }

        public synchronized byte[] getBackedByteArray() {
            return buf;
        }

        public synchronized int size() {
            return this.count;
        }
    }

}
