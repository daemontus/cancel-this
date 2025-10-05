# PyO3 functionality example

This example project shows how to use `cancel_this` with PyO3. It is something that can't really be easily
shown using code examples in Rust documentation.

## Running the example

1. Create a virtual Python environment (`python3 -m venv ./venv`) and activate it (`source ./venv/bin/activate`).
2. Install maturin (`pip install maturin`).
3. Build the example project (`maturin develop --release`).
4. Run `python3 test.py`.

The test first computes a simple hash function (with artificial delays so that we can keep the data small)
using an uncancellable function. The function should run for ~4-5s. You can verify that it cannot be cancelled
by pressing Ctrl+C while it is running (this will cancel the computation once the uncancellable block of code is
completed). Afterward, the same operation is executed using a cancellable function. You can again verify that 
in this case, the operation can be cancelled using Ctrl+C. 

During the first operation, the liveness guard should report
that the computation has become unresponsive. As soon as the second operation starts, a new message should be
printed that the computation is responsive again.

**Note on liveness:** Our use of liveness guard here may not always be optimal. 
In practice, we may want to create a separate liveness guard for each operation directly in the native code. 
Since cancellation is not checked in Python code, a liveness guard that outlives all "native" computations will 
eventually simply be marked as unresponsive, because all code execution is happening in the Python interpreter. 
Furthermore, we can't really force the interpreter to destroy the liveness guard (we can request garbage collection, 
but this is not 100% guaranteed to work).