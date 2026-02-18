package io.github.liyze09.nexus.render;

import com.mojang.blaze3d.buffers.GpuBufferSlice;
import com.mojang.blaze3d.pipeline.RenderTarget;
import com.mojang.blaze3d.resource.GraphicsResourceAllocator;
import io.github.liyze09.nexus.NexusBackend;
import io.github.liyze09.nexus.chunk.NexusChunkBuilder;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.CloudRenderer;
import net.minecraft.client.renderer.LevelRenderer;
import net.minecraft.client.renderer.RenderBuffers;
import net.minecraft.client.renderer.SectionOcclusionGraph;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderDispatcher;
import net.minecraft.client.renderer.chunk.SectionRenderDispatcher;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.feature.FeatureRenderDispatcher;
import net.minecraft.client.renderer.state.LevelRenderState;
import net.minecraft.core.BlockPos;
import net.minecraft.gizmos.Gizmos;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.state.BlockState;
import org.jspecify.annotations.Nullable;
import org.joml.Matrix4f;
import org.joml.Vector4f;
import org.jspecify.annotations.NonNull;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;

public final class NexusWorldRenderer extends LevelRenderer {
    private static final Logger LOGGER = LoggerFactory.getLogger(NexusWorldRenderer.class);
    public final NexusBackend backend;
    private final Minecraft minecraft;
    public volatile NexusChunkBuilder builder = null;
    @Nullable
    private volatile ClientLevel world = null;
    private ExternalImageRenderer renderer;
    private volatile boolean isCompleted = true;
    private int width;
    private int height;
    private boolean rendering = false;
    private volatile boolean isClosed = false;
    private ResourceManager resourceManager;

    public NexusWorldRenderer(Minecraft minecraft, EntityRenderDispatcher entityRenderDispatcher, BlockEntityRenderDispatcher blockEntityRenderDispatcher, RenderBuffers renderBuffers, LevelRenderState levelRenderState, FeatureRenderDispatcher featureRenderDispatcher) {
        super(minecraft, entityRenderDispatcher, blockEntityRenderDispatcher, renderBuffers, levelRenderState, featureRenderDispatcher);
        this.backend = new NexusBackend();
        this.minecraft = minecraft;
        this.resourceManager = minecraft.getResourceManager();
        reloadResources(resourceManager, backend);
        this.width = minecraft.getWindow().getWidth();
        this.height = minecraft.getWindow().getHeight();
        backend.resize(width, height);
        long r = backend.getGLReady();
        long c = backend.getGLComplete();
        this.renderer = new ExternalImageRenderer(r, c, backend.getGLTexture(), width, height, backend.getRenderTargetSize());
        LOGGER.info("NexusWorldRenderer created.");
    }

    private static void reloadResources(ResourceManager resourceManager, NexusBackend backend) {

    }

    @Override
    public void close() {
        checkIfClosed();
        super.close();
        isClosed = true;
        renderer.cleanup();
        // backend.close();
    }

    public boolean isClosed() {
        return isClosed;
    }

    @Nullable
    public ClientLevel getWorld() {
        return world;
    }

    private void checkIfClosed() {
        if (isClosed) {
            throw new IllegalStateException("Renderer has already closed.");
        }
    }

    @Override
    public @NonNull CompletableFuture<Void> reload(@NonNull SharedState sharedState, @NonNull Executor executor, @NonNull PreparationBarrier preparationBarrier, @NonNull Executor executor2) {
        return super.reload(sharedState, executor, preparationBarrier, executor2);
    }

    @Override
    public void prepareSharedState(@NonNull SharedState sharedState) {
        super.prepareSharedState(sharedState);
    }

    @Override
    public @NonNull String getName() {
        return super.getName();
    }

    @Override
    public void onResourceManagerReload(@NonNull ResourceManager resourceManager) {
        this.resourceManager = resourceManager;
        reloadResources(resourceManager, backend);
    }

    @Override
    public void initOutline() {
    }

    @Override
    public void doEntityOutline() {
    }

    @Override
    protected boolean shouldShowEntityOutlines() {
        return false;
    }

    @Override
    public void setLevel(@Nullable ClientLevel clientLevel) {
        this.world = clientLevel;
        if (clientLevel != null) {
            this.builder = new NexusChunkBuilder(clientLevel, backend);
            if (!rendering) {
                this.rendering = true;
            }
        } else {
            this.builder = null;
            if (rendering) {
                this.rendering = false;
            }
        }

    }

    @Override
    public void allChanged() {
    }

    @Override
    public void resize(int i, int j) {
        this.width = i;
        this.height = j;
        backend.resize(width, height);
        this.renderer.cleanup();
        this.renderer = new ExternalImageRenderer(backend.getGLReady(), backend.getGLComplete(), backend.getGLTexture(), width, height, backend.getRenderTargetSize());
    }

    @Override
    public @Nullable String getSectionStatistics() {
        return null;
    }

    @Override
    public @Nullable SectionRenderDispatcher getSectionRenderDispatcher() {
        return null;
    }

    @Override
    public double getTotalSections() {
        return 0;
    }

    @Override
    public double getLastViewDistance() {
        return 0;
    }

    @Override
    public int countRenderedSections() {
        return 0;
    }

    @Override
    public void resetSampler() {
    }

    @Override
    public @Nullable String getEntityStatistics() {
        return null;
    }

    @Override
    public void addRecentlyCompiledSection(SectionRenderDispatcher.@NonNull RenderSection renderSection) {
    }

    @Override
    public void renderLevel(@NonNull GraphicsResourceAllocator graphicsResourceAllocator, @NonNull DeltaTracker deltaTracker, boolean bl, @NonNull Camera camera, @NonNull Matrix4f matrix4f, @NonNull Matrix4f matrix4f2, @NonNull Matrix4f matrix4f3, @NonNull GpuBufferSlice gpuBufferSlice, @NonNull Vector4f vector4f, boolean bl2) {
        checkIfClosed();
        this.isCompleted = false;
        renderer.render(backend::render);
        var target = this.minecraft.getMainRenderTarget();
        target.colorTexture = renderer.blaze3dTexture;
        target.colorTextureView = renderer.blaze3dTextureView;
        this.isCompleted = true;
    }

    @Override
    public void endFrame() {
    }

    @Override
    public void captureFrustum() {
    }

    @Override
    public void killFrustum() {
    }

    @Override
    public void tick(@NonNull Camera camera) {
    }

    @Override
    public void blockChanged(@NonNull BlockGetter blockGetter, @NonNull BlockPos blockPos, @NonNull BlockState blockState, @NonNull BlockState blockState2, int i) {
    }

    @Override
    public void setBlocksDirty(int i, int j, int k, int l, int m, int n) {
    }

    @Override
    public void setBlockDirty(@NonNull BlockPos blockPos, @NonNull BlockState blockState, @NonNull BlockState blockState2) {
    }

    @Override
    public void setSectionDirtyWithNeighbors(int i, int j, int k) {
    }

    @Override
    public void setSectionRangeDirty(int i, int j, int k, int l, int m, int n) {
    }

    @Override
    public void setSectionDirty(int i, int j, int k) {
    }

    @Override
    public void onSectionBecomingNonEmpty(long l) {
    }

    @Override
    public void destroyBlockProgress(int i, @NonNull BlockPos blockPos, int j) {
    }

    @Override
    public boolean hasRenderedAllSections() {
        return isCompleted;
    }

    @Override
    public void onChunkReadyToRender(@NonNull ChunkPos chunkPos) {
    }

    @Override
    public void needsUpdate() {
    }

    @Override
    public boolean isSectionCompiledAndVisible(@NonNull BlockPos blockPos) {
        return true;
    }

    @Override
    public @Nullable RenderTarget entityOutlineTarget() {
        return null;
    }

    @Override
    public @Nullable RenderTarget getTranslucentTarget() {
        return null;
    }

    @Override
    public @Nullable RenderTarget getItemEntityTarget() {
        return null;
    }

    @Override
    public @Nullable RenderTarget getParticlesTarget() {
        return null;
    }

    @Override
    public @Nullable RenderTarget getWeatherTarget() {
        return null;
    }

    @Override
    public @Nullable RenderTarget getCloudsTarget() {
        return null;
    }

    @Override
    public @NonNull ObjectArrayList<SectionRenderDispatcher.RenderSection> getVisibleSections() {
        return new ObjectArrayList<>();
    }

    @Override
    public @NonNull SectionOcclusionGraph getSectionOcclusionGraph() {
        return super.getSectionOcclusionGraph();
    }

    @Override
    public @Nullable Frustum getCapturedFrustum() {
        return super.getCapturedFrustum();
    }

    @Override
    public @NonNull CloudRenderer getCloudRenderer() {
        return super.getCloudRenderer();
    }

    @Override
    public Gizmos.@NonNull TemporaryCollection collectPerFrameGizmos() {
        return super.collectPerFrameGizmos();
    }
}
