package io.github.liyze09.nexus.chunk;

import io.github.liyze09.nexus.NexusBackend;
import io.github.liyze09.nexus.model.block.CubeModel;
import io.github.liyze09.nexus.model.block.ModelManager;
import io.github.liyze09.nexus.model.block.VisibleFaces;
import io.github.liyze09.nexus.utils.LayeredBlockGetter;
import net.minecraft.client.Camera;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.phys.shapes.CollisionContext;
import org.jspecify.annotations.Nullable;
import org.jspecify.annotations.NonNull;

import java.util.ArrayList;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicReferenceArray;

public class NexusChunkBuilder {
    final Map<ChunkPos, LevelChunk> loadedChunks;
    final Map<SectionPos, BuiltSection> builtSections;
    final NexusBackend backend;
    ClientLevel world;
    AtomicReferenceArray<LevelChunk> chunks;
    Camera camera;
    CollisionContext collisionContext = CollisionContext.empty();

    public NexusChunkBuilder(@NonNull ClientLevel world, @NonNull NexusBackend backend) {
        this.world = world;
        this.backend = backend;
        this.chunks = world.getChunkSource().storage.chunks;
        this.loadedChunks = new ConcurrentHashMap<>(chunks.length());
        this.builtSections = new ConcurrentHashMap<>(chunks.length() * 16);
        rebuild0(chunks, loadedChunks);
    }

    private static void rebuild0(@NonNull AtomicReferenceArray<LevelChunk> chunks, Map<ChunkPos, LevelChunk> loadedChunks) {
        for (int i = 0; i < chunks.length(); i++) {
            LevelChunk chunk = chunks.get(i);
            if (chunk != null) {
                loadedChunks.put(chunk.getPos(), chunk);
            }
        }
    }

    public void setCamera(@Nullable Camera camera) {
        this.camera = camera;
        this.collisionContext = camera == null ? CollisionContext.empty() : CollisionContext.of(camera.entity());
    }

    public void load(LevelChunk chunk) {
        if (chunk != null) {
            loadedChunks.put(chunk.getPos(), chunk);
        }
    }

    public void unload(@NonNull LevelChunk chunk) {
        var chunkPos = chunk.getPos();
        for (int i = 0; i < chunk.getSections().length; i++) {
            var sectionPos = SectionPos.of(chunkPos, i);
            if (builtSections.containsKey(sectionPos)) {
                builtSections.remove(sectionPos).close();
            }
        }
        loadedChunks.remove(chunkPos);
    }

    public void rebuild(@NonNull ClientLevel world) {
        this.loadedChunks.clear();
        this.builtSections.clear();
        rebuild0(world.getChunkSource().storage.chunks, this.loadedChunks);
    }

    public boolean isSectionBuilt(@NonNull BlockPos blockPos) {
        return builtSections.containsKey(SectionPos.of(blockPos));
    }

    public void build(SectionPos sectionPos) {
        var chunk = loadedChunks.get(new ChunkPos(sectionPos.x(), sectionPos.z()));
        var section = chunk.getSection(sectionPos.y());
        var sparseBlocks = new ArrayList<SparseBlock>();
        sectionPos.blocksInside().forEach(blockPos -> {
            var blockState = chunk.getBlockState(blockPos);
            if (blockState.isAir()) {
                return;
            }
            var model = ModelManager.getInstance().getModel(blockState);
            var blockGetter = new LayeredBlockGetter(world, chunk, section, sectionPos.y());
            var visibleFaces = new VisibleFaces();
            visibleFaces.up    = blockGetter.getBlockState(blockPos.above()).canOcclude();
            visibleFaces.down  = blockGetter.getBlockState(blockPos.below()).canOcclude();
            visibleFaces.north = blockGetter.getBlockState(blockPos.north()).canOcclude();
            visibleFaces.south = blockGetter.getBlockState(blockPos.south()).canOcclude();
            visibleFaces.west  = blockGetter.getBlockState(blockPos.west ()).canOcclude();
            visibleFaces.east  = blockGetter.getBlockState(blockPos.east ()).canOcclude();
            if (visibleFaces.isAnyVisible()) {
                if (model instanceof CubeModel) {
                    sparseBlocks.add(new SparseBlock((byte) blockPos.getX(), (byte) blockPos.getY(), (byte) blockPos.getZ(),
                            model.getModelId(), visibleFaces.toFaceMask()));
                } else {
                    sparseBlocks.add(new SparseBlock((byte) blockPos.getX(), (byte) blockPos.getY(), (byte) blockPos.getZ(),
                            model.getModelId(), (byte) 0));
                }
            }
        });
        this.builtSections.put(sectionPos, new BuiltSection(sectionPos, sparseBlocks));
    }
}
