# Page count in file header
I have to implement `Pager` <- interface to deal with page level IO.

It takes page out and evict if needed.
But we need a way to tell how many page are there in the 
file. so we gotta add that to file header. 

# Root id in file header
I just realize in B-tree, root can always change (it grows up).
So I think we need to also store root id in header.

Now file header is
- Magic number like: `0x0123456789abcdef` first
- Then row schema. should be
    1. How many columns in `u32`
    2. Column info entries, each entry is
        1. Column name: 
            1. string length `u16` in bytes 
            2. real string in UTF-8
        2. Column type: enum `u8`
- Page count: how many pages are there in `u32` 
    - 1 page = 1KB, 2^32 pages = 20TB, so I think `u32` make sense
- Root node id: `u32`

