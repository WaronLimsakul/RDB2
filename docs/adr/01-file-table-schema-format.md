# Table header format

At the start of `.rdb` file, it should be

- Magic number like: `0x0123456789abcdef` first
- Then row schema. should be
    1. How many columns in `u32`
    2. Column info entries, should be like
        1. Column name: 
            1. string length `u32` in bytes 
            2. real string in UTF-8
        2. Column type: enum `u32`
        - NOTE: First column should be something like ID, so I'll force `u32`
    

