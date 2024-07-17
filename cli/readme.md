# How to compile and use cli tool

Before you compile the cli tool source, you should **move cli directory to a location outside the root directory of rust-shyper**. Because cli and rust-shyper are two independent rust crates, and will have conflicts when compiling.

First, create a Linux distribution environment for one architecture (AArch64 or RISCV64).

Second, apt install the following dependency under this environment:

* libfdisk-dev

Third, locate the directory to the cli, then run `make`.

Finally, you can use the compiled executable as a command-line tool to manage rust-shyper.
