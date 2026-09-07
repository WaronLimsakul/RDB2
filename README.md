# RDB2

A basic relational database implementation by Ron.

## Features

- Disk-backed tables indexed by a primary-key B-tree
- `new table`, `insert`, `select ... where`, `delete table`
- Filtering with `=`, `!=`, `<`, `<=`, `>`, `>=`, combined with `&&`
- REPL with line history, pretty-printed table output, and meta commands

## Quick start

```sh
cargo run
```

## Usage

For now, RDB2 is a REPL: type a command and press Enter. Keywords and types are case-insensitive.

### Meta commands

| Command      | What it does                         |
|--------------|--------------------------------------|
| `.tables`    | List all tables (`ls` also works)    |
| `.quit`      | Exit, saving data to disk (`exit`)   |

### Create a table

```sql
new table User { id: uint primary, name: string, age: uint };
```

One column must be marked `primary`, and its type must be `uint` or `ulong`.
Other column types are `int`, `long`, `float`, `bool`, `string`.

### Insert rows

```sql
insert User (1, "Alice", 30);
insert User [(2, "Bob", 25), (3, "Charlie", 35)];
```

Values are positional: primary key first, then the other columns in the order
you declared them.

### Query

```sql
select * from User;
select name, age from User where age > 25 && age <= 35;
```

`*` selects every column. A `where` clause keeps rows where all predicates
hold; a column or a literal may appear on either side of a comparison.

### Drop a table

```sql
delete table User;
```

## Next steps

- Joins (multi-table queries) and column-name qualification
- Row-level `update` / `delete`, secondary indexes
- Transactions and a write-ahead log (data is currently flushed on clean exit)
- Serve over a protocol instead of the REPL
