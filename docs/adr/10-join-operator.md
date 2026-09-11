- Well, should be just multiple table + cross column predicate
- So normal join should be like:
```
select * from A, B where id = a_id;
```

When `id` is in `A` and `a_id` is in `B`.

- For this, I have to add another method for `Operator` trait
- I need `rewind`, so that my `Cartesian` product operator can rewind the table source keep doing cartesian product
- The next problem is the ownership of the engine when create multiple `Scan`s at a time, I'll just get multiple `&mut Table`s from engine in one operations (we can do this because we get *different* tables)

- Also, we have to introduce the "ambiguity" detection when user mention a column that appear multiple time in an output.

