struct ScreenVertex {
    f16vec2 xy; 
    float depth; 
};

struct ScreenCluster {
    uint start; 
};

struct ScreenQuad {
    uint cluster;
    u8vec4 vertexs;
    uint material; 
};

struct TileInput {
    uint start; 
    uint count;
};