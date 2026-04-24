# B-tree node bytes format

I'm thinking abstract node, meaning I'll do everything
I can from RAM, then just flush to disk after I figure out what to write.

First, I think 1 node = 1 page = 4096 bytes.

Then, basic node struct (on RAM) should have:
1. Node ID: `u32` <- should I have this? not sure, will keep it.
2. If the page is dirty so that we can check when flush: `bool`
3. If the node is leaf: `bool`
4. The last pointer for internal node (`[p k] [p k] .. [p k] p!`): `u32`
5. Cells: `Vec<Cell>`. `Cell` should be
    1. Key: `u64` <- B+-tree key, bound for record id
    2. Value: depends on if it's leaf or internal
        - Internal: just `u32` <- point to other node id
        - Leaf: Depends on schema: `(String, TypeData)`


On disk, I'm thinking slotted page, so something like:
1. Header at the start. should have
    1. Node ID: `u32`
    2. Magic Number: something like 0x 50 41 47 45 (for P A G E): `u32`
    3. Is Leaf: `u8` boolean (0 if no, other if yes)
        - Note: can bunch some other bool type data here
    4. Cells count: `u16` 
    5. Free space pointer: `u16`
    6. Sibling node id: `u32`
2. Pointers: bunch of `u16` offseting itself with the cell
3. Cells from the back. Should hold real record: depends
    - Internal node = key cell
        - key size in bytes: `u32` <- force id `u64`, but in case I change
        - child pointer: `u32`
        - key: depends but should be `u64`
    - Leaf node = kv cell
        - key size in bytes: `u32` <- same as before, in case I change
        - value size in bytes: `u32`
        - key: depends, but default `u64`
        - record: depends on data

## How to encode record data?
First, let's just use Big-endian.

Then, I think I'll do `string = [u16, [u8]]`
- First part tell length
- Second part real data UTF-8

Other than that, it's fixed-size, so should be fine.

## Overflow?
TODO. Don't wanna deal with this now
