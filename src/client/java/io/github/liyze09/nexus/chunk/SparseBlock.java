package io.github.liyze09.nexus.chunk;

public class SparseBlock {
    public final byte localX, localY, localZ; // Highest 12-bit
    public final int blockState; // Lowest 22-bit
    public final byte faceMask; // 6-bit mask: up, down, north, south, east, west (LSB to MSB)
    public SparseBlock(byte localX, byte localY, byte localZ, int blockState, byte faceMask) {
        this.localX = localX;
        this.localY = localY;
        this.localZ = localZ;
        this.blockState = blockState;
        this.faceMask = faceMask;
    }
    public long encodeAsLong() {
        return ((long)(localX & 0xF) << 36) | ((long)(localY & 0xF) << 32) | ((long)(localZ & 0xF) << 28)
                | ((long)(faceMask & 0x3F) << 22) | (blockState & 0x3FFFFF);
    }

    public byte[] encodeAsBytes() {
        var ret = new byte[5];
        ret[0] = (byte) ((localX & 0b00001111) << 4 | (localY & 0b00001111));
        ret[1] = (byte) ((localZ & 0b00001111) << 4 | (faceMask & 0b00111111) >> 2);
        ret[2] = (byte) ((faceMask & 0b00000011) << 6 | (blockState & 0b1111110000000000000000) >> 16);
        ret[3] = (byte) (blockState & 0b0000001111111100000000 >> 8);
        ret[4] = (byte) (blockState & 0b11111111);
        return ret;
    }
}