package io.github.liyze09.nexus.model.block;

public class VisibleFaces {
    public boolean north = true;
    public boolean east = true;
    public boolean south = true;
    public boolean west = true;
    public boolean up = true;
    public boolean down = true;

    public VisibleFaces(boolean north, boolean south, boolean west,
                        boolean east, boolean up, boolean down) {
        this.north = north;
        this.south = south;
        this.west = west;
        this.east = east;
        this.up = up;
        this.down = down;
    }

    public VisibleFaces() {

    }

    public boolean isAnyVisible() {
        return north || south || west || east || up || down;
    }

    public byte toFaceMask() {
        byte mask = 0;
        if (up) mask |= 1;
        if (down) mask |= 2;
        if (north) mask |= 4;
        if (south) mask |= 8;
        if (east) mask |= 16;
        if (west) mask |= 32;
        return mask;
    }
}
