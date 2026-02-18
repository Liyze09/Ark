module common; 

public struct SectionHeader {
    private uint64_t flag;
    public int offset;

    public property x : int {
        get {
            return int(flag & 0x3FFFFF);
        }
    }

    public property y : int8_t {
        get {
            return int8_t((flag >> 44) & 0xFF);
        }
    }

    public property z : int {
        get {
            return int((flag >> 22) & 0x3FFFFF);
        }
    }

    public property blockCount : int16_t {
        get {
            return int16_t((flag >> 52) & 0xFFF);
        }
    }
}

public struct SparseBlock {
    private uint8_t bytes[5];

    public property uint8_t localX { get { return (bytes[0] >> 4) & 0xF; } }
    public property uint8_t localY { get { return bytes[0] & 0xF; } }
    public property uint8_t localZ { get { return (bytes[1] >> 4) & 0xF; } }
    public property int blockState {
        get {
            return ((bytes[2] & 0x3F) << 16) | (bytes[3] << 8) | bytes[4];
        }
    }

    static const uint UP = 0;    // bit0
    static const uint DOWN = 1;  // bit1
    static const uint NORTH = 2; // bit2
    static const uint SOUTH = 3; // bit3
    static const uint EAST = 4;  // bit4
    static const uint WEST = 5;  // bit5

    public property bool up { get {
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << UP)) != 0;
    }}
    public property bool down { get {
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << DOWN)) != 0;
    }}
    public property bool north { get { 
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << NORTH)) != 0;
    }}
    public property bool south { get { 
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << SOUTH)) != 0;
    }}
    public property bool east { get {
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << EAST)) != 0;
    }}
    public property bool west { get {
        uint8_t mask = (bytes[1] & 0xF) | ((bytes[2] >> 6) << 4);
        return (mask & (1u << WEST)) != 0;
    }}
}