package io.github.liyze09.nexus.utils;

import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.material.FluidState;
import org.jspecify.annotations.Nullable;
import org.jspecify.annotations.NonNull;

import static io.github.liyze09.nexus.utils.NexusUtils.*;

public class LayeredBlockGetter implements BlockGetter {
    private final Level world;
    private final LevelChunk chunk;
    private LevelChunkSection section;
    private BlockPos origin;
    private BlockPos end;

    public LayeredBlockGetter(@NonNull Level world, @NonNull LevelChunk chunk, @NonNull LevelChunkSection section, int sectionIndex) {
        this.world = world;
        this.section = section;
        this.chunk = chunk;
        this.origin = SectionPos.of(chunk.getPos(), sectionIndex).origin();
        this.end = this.origin.offset(15, 15, 15);
    }

    public LayeredBlockGetter(@NonNull Level level, @NonNull LevelChunk chunk) {
        this.world = level;
        this.chunk = chunk;
    }

    public void setSection(@NonNull LevelChunkSection section, int sectionIndex) {
        this.section = section;
        this.origin = SectionPos.of(chunk.getPos(), sectionIndex).origin();
        this.end = this.origin.offset(15, 15, 15);
    }

    @Override
    public @Nullable BlockEntity getBlockEntity(@NonNull BlockPos blockPos) {
        if (inChunk(blockPos, chunk)) {
            return chunk.getBlockEntity(blockPos);
        }
        return world.getBlockEntity(blockPos);
    }

    @Override
    public @NonNull BlockState getBlockState(@NonNull BlockPos blockPos) {
        if (world.isDebug()) {
            return world.getBlockState(blockPos);
        } else if (inArea(blockPos, origin, end)) {
            return section.getBlockState(blockPos.getX() - origin.getX(), blockPos.getY() - origin.getY(), blockPos.getZ() - origin.getZ());
        } else if (inChunk(blockPos, chunk)) {
            return chunk.getBlockState(blockPos);
        } else {
            return world.getBlockState(blockPos);
        }
    }

    @Override
    public @NonNull FluidState getFluidState(@NonNull BlockPos blockPos) {
        if (inArea(blockPos, origin, end)) {
            return section.getFluidState(blockPos.getX() - origin.getX(), blockPos.getY() - origin.getY(), blockPos.getZ() - origin.getZ());
        } else if (inChunk(blockPos, chunk)) {
            return chunk.getFluidState(blockPos);
        } else {
            return world.getFluidState(blockPos);
        }
    }

    @Override
    public int getHeight() {
        return world.getHeight();
    }

    @Override
    public int getMinY() {
        return world.getMinY();
    }
}
