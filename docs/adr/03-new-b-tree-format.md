# Ok, the abstract page is stupid idea

Everytime I have to flush, I have to recalculate and serial everything again.
What's even the point of slotted page then?

So I'm thinking maybe I'll keep a page being just `[u8; 4096]`
then I'll implement methods on in it. That's it.

On-disk format is almost the same tho:
1. Header at the start. should have
    1. Magic Number: something like 0x 50 41 47 45 (for P A G E): `u32`
    2. Node ID: `u32`
    3. Is Leaf: `u8` boolean (0 if no, other if yes)
        - Note: can bunch some other bool type data here
    4. Cells count: `u16` 
    5. Free space pointer: `u16`
    6. Sibling node id: `u32`
    7. Rightmost value: `u32`
2. Pointers: bunch of `u16` offsets from start of page to cell
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
