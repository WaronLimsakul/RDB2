- Sometimes, when query with many tables (join), table might have same column name. 
- We want user to specify which column they mean.
- So we will allow `<table>.<column>` to be valid column statement.
- Thus user can write something like

```sql
select t1.c1, t2.c1 from t1, t2 where t1.id = t2.tid;
```
- If not because of this new qualification, we would have not able to project `c1` from both `t1` and `t2`
- NOTE: the label user use to project will be the one displayed of the output, not included the predicate. 

