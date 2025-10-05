from example_py import hash_data, hash_data_unchecked, Liveness
import time

data = [ x for x in range(1 << 12) ]

print(f"Created data buffer with {len(data)} elements.")

guard = Liveness()

# During this computation, the script should become "unresponsive".
start = time.time()
hash = hash_data_unchecked(data)
end = time.time()
elapsed = end - start

print(f"Data hashed (unsafe) to {hash} in {elapsed * 1000}ms.")

# And here, it should be "responsive" again, due to cancellation checks.
start = time.time()
hash = hash_data(data)
end = time.time()
elapsed = end - start

print(f"Data hashed to {hash} in {elapsed * 1000}ms.")