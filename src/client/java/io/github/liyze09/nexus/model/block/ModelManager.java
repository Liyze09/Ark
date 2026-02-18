package io.github.liyze09.nexus.model.block;

import net.minecraft.world.level.block.state.BlockState;
import org.jetbrains.annotations.NotNull;

import java.util.HashMap;

public class ModelManager {
    private static final ModelManager instance = new ModelManager();
    public HashMap<BlockState, BakedBlock> map = new HashMap<>();

    private ModelManager() {
        // TODO: load models
    }

    public static ModelManager getInstance() {
        return instance;
    }

    private static final BakedBlock DEFAULT_MODEL = new BakedBlock() {
        @Override
        public int getModelId() {
            return 0;
        }

        @Override
        public Model getModel() {
            return CubeModel.INSTANCE;
        }
    };

    @NotNull
    public BakedBlock getModel(BlockState state) {
        return map.getOrDefault(state, DEFAULT_MODEL);
    }
}
