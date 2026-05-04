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

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.OutputStream;
import java.lang.foreign.MemorySegment;
import java.nio.ByteBuffer;
import java.util.concurrent.CompletableFuture;

public class ServerLevelBinaryChunkSource implements BinaryChunkSource {
    private final IOWorkerAccessor worker;

    public ServerLevelBinaryChunkSource(@NonNull ServerLevel level) {
        this.worker = (IOWorkerAccessor) ((SimpleRegionStorageAccessor) level.getChunkSource().chunkMap).getIOWorker();
    }

    @Override
    public CompletableFuture<MemorySegment> getBinaryChunk(ChunkPos pos) {
        return submitThrowingTask(() -> {
            var pendingStore = worker.getPendingWrites().get(pos);
            if (pendingStore != null) {
                CompoundTag data = pendingStore.data;
                if (data == null) {
                    return null;
                }

                var buf = new DirectByteBufferOutputStream(8192);
                RegionFile regionFile = worker.getRegionFileStorage().getRegionFile(pos);
                DataOutputStream regionOut = regionFile.getChunkDataOutputStream(pos);
                try (TeeOutputStream tee = new TeeOutputStream(regionOut, buf);
                     DataOutputStream dataOut = new DataOutputStream(tee)) {
                    NbtIo.write(data, dataOut);
                }
                return MemorySegment.ofBuffer(buf.takeBuffer());
            }

            RegionFile regionFile = worker.getRegionFileStorage().getRegionFile(pos);
            DataInputStream chunkStream = regionFile.getChunkDataInputStream(pos);
            if (chunkStream == null) {
                return null;
            }
            var buf = new DirectByteBufferOutputStream(8192);
            try (chunkStream) {
                chunkStream.transferTo(buf);
            }
            return MemorySegment.ofBuffer(buf.takeBuffer());
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

    private static final class DirectByteBufferOutputStream extends OutputStream {
        private ByteBuffer buf;

        DirectByteBufferOutputStream(int initialCapacity) {
            buf = ByteBuffer.allocateDirect(initialCapacity);
        }

        @Override
        public void write(int b) {
            ensureCapacity(1);
            buf.put((byte) b);
        }

        @Override
        public void write(byte @NonNull [] src, int off, int len) {
            ensureCapacity(len);
            buf.put(src, off, len);
        }

        private void ensureCapacity(int needed) {
            if (buf.remaining() < needed) {
                int newCap = Math.max(buf.capacity() * 2, buf.position() + needed);
                ByteBuffer newBuf = ByteBuffer.allocateDirect(newCap);
                buf.flip();
                newBuf.put(buf);
                buf = newBuf;
            }
        }

        ByteBuffer takeBuffer() {
            buf.flip();
            ByteBuffer result = buf;
            buf = null;
            return result;
        }

        @Override
        public void close() {
        }
    }
}
