I want auto-incremented ID when people insert, so they don't have to track the latest ID.

It should be defined when create a table, will only support it in primary key for now. Something like

```
new table Foo {
    id: uint PRIMARY AUTO,
    ...
};
```
- `AUTO` means auto-increment

But this means we have to modify disk-level encoding to enable auto-incrementing flag. So I'll add 
1 byte to the column entry to enable 8 bit flags (1 used for now, but may be used in the future).

