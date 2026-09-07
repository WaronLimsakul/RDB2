- Inside the `select` statement, there should be filtering option
- Will use `where` keyword
- For now, force one side to be column name, and another side to be literal expression
- Can chain the condition, for now, only allow 'and' operator, represented by `&&` symbol

E.g.
```
select * from Foo where age > 20 && is_cool = true;
```
