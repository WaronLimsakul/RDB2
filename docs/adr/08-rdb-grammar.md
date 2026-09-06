4 main functions we need
1. Create table
2. Insert values
3. Get values
4. Drop table

## Create table
Let's do something like:

```
new table <table_name> {
    <col1_name>: <col1_type> primary,
    <col2_name>: <col2_type>,
    ...
};
```
- Type can be: `int`, `uint`, `long`, `ulong`, `string`, `bool` 
- One of them must be primary, and should be of the type we support: `uint` or `ulong`

## Insert values
Just insert that's it
```
insert <table_name> [
    (<col1_val>, <col2_val>, ...),
    (<col1_val>, <col2_val>, ...),
    ...
];
```
or, for one row 
```
insert <table_name> (<col1_val>, <col2_val>, ...);

```

## Get values
Select, I guess.
```
select <coli_name>, <colj_name>, ...
from <table_name>;
```
- Can use `*` as all columns
- Will deal with predicate and aggregate later.


## Drop table
I like `delete`.
```
delete table Foo;
```
That's it.

