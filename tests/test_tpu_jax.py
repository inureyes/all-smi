import os
import jax
import jax.numpy as jnp
import numpy as np

def test_tpu_setup():
    print("=== JAX TPU Environment Check ===")
    
    # 1. Check JAX backend and platform
    print(f"JAX version: {jax.__version__}")
    print(f"Default backend: {jax.default_backend()}")
    
    # 2. Verify connected TPU devices
    try:
        devices = jax.devices()
        print(f"Number of connected devices: {len(devices)}")
        for i, d in enumerate(devices):
            print(f" - Device {i}: {d.device_kind} (ID: {d.id})")
    except Exception as e:
        print(f"❌ Failed to find devices: {e}")
        return

    # 3. Simple matrix multiplication test (Verify TPU acceleration)
    print("\n=== Computation Test Started ===")
    try:
        # Generate random numbers (performed on TPU)
        key = jax.random.PRNGKey(42)
        size = 3000
        x = jax.random.normal(key, (size, size), dtype=jnp.float32)
        y = jax.random.normal(key, (size, size), dtype=jnp.float32)

        # Perform Matrix Multiplication (Matmul)
        # Note: The first execution includes XLA compilation time.
        print(f"Performing {size}x{size} matrix multiplication...")
        result = jnp.matmul(x, y)
        
        # Verify the result (use block_until_ready to wait for async TPU execution)
        print(f"Connection Successful! Sample output: {result[0, :5].block_until_ready()}")
        print("✅ TPU computation completed successfully.")
        
    except Exception as e:
        print(f"❌ Error during computation: {e}")

if __name__ == "__main__":
    test_tpu_setup()
