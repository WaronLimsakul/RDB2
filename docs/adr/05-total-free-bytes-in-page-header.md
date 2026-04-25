# Total free bytes in page header format
I forget to think about the free space management in the slotted page.

- We have what I call `front_ptr` which points to the back of pointers
- Then we have `back_ptr` stored in header in the name `free_space_ptr`

These 2 are enough to tell how much space we have left. However,
I forget that when we delete, we might simple just mark something
as deleted (have to decide later), so we won't know how much 
in total of free space we have. Therefore, we have to store
another number `total_free_space: u16`.

I'll put it right after the `free_space_ptr`

