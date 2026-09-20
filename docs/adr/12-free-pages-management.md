From the `delete` feature, each page now is able to be merged away (or deleted, if you will). However, we can't just shift every node to fit the deleted page, that would be super inefficient. So we should

- Mark the node as "free"
- Keep track of the free nodes in our file
- Reuse that free node when allocate new node whenever possible.

How do we keep track of free nodes? We can't just use a list in the header, because it's dynamically sized. I'll use linked list.

- File header hold ID of the first free page list
- Each free page has its own "free page magic number" written at its header
- It also have ID of the next free page, so on and so fourth.


