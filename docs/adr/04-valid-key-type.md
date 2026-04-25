# Allowed type for record's key (id)

I initially think that I will force `Ulong` to be only type for id and will
change this later, but I feel like this will need a lot of work if I don't
do it now. So here it is.

2 choices:
1. I can still use same `Type` then implement sth like `valid_key_type()` method
but this is not clear when I return something that suppose to be key type.
2. New enum type that associated with the old type, used when only talk about key

I'll choose 2, I want it to be as clear as possible.

So now a node On-disk format is:
1. Header at the start. should have
    1. Magic Number: something like 0x 50 41 47 45 (for P A G E): `u32`
    2. Node ID: `u32`
    3. Flags: `u8` but bunch of flags
        1. 1-6
        2. 7th bit: is leaf?
        3. 8th bit: is root?
    4. Cells count: `u16` 
    5. Free space pointer: `u16` <- point to first free byte from the back (for cells)
    6. Sibling node id: `u32`
    7. Rightmost value: `u32`
2. Pointers: bunch of `u16` offsets from start of page to cell
3. Cells from the back. Should hold real record: depends
    - Internal node = key cell
        - key size in bytes: `u8` <- shouldn't be a key > 128 bytes anyway
        - child pointer: `u32`
        - key: depends on key size
    - Leaf node = kv cell
        - key size in bytes: `u8`
        - value size in bytes: `u32`
        - key: depends on key size
        - record: depends on data
