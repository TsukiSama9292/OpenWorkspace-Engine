# gVisor (runsc) — GPU Sandboxing

gVisor is a user-space kernel that intercepts system calls and runs them in an
isolated, host-protecting sandbox. In OpenWorkspace, templates can set
**Container Runtime → gVisor (`runsc`)** so a tenant's high-risk syscalls never
reach the host kernel directly.

For GPU work, gVisor ships **NVProxy** (`runsc --nvproxy`): it intercepts the
container's NVIDIA driver ioctls and proxies them to the host driver. The
container never gets raw device access, and the sandbox still sees CUDA.

## GPU Hardware Compatibility

### Officially supported (per [gVisor docs](https://gvisor.dev/docs/user_guide/gpu/))

gVisor currently supports these NVIDIA GPUs:

| GPU | Microarchitecture |
|---|---|
| **T4** | Turing |
| **A100 / A10G** | Ampere |
| **L4** | Ada Lovelace |
| **H100** | Hopper |

> While not officially supported, other NVIDIA GPUs based on the **same
> microarchitectures** as the above will likely work as well. This includes
> consumer-oriented GPUs such as **RTX 3090** (Ampere) and **RTX 4090**
> (Ada Lovelace).

So the deciding factor is the **microarchitecture**, not the marketing series.
Turing, Ampere, Ada Lovelace, and Hopper are covered; older architectures are
not.

### Verified on our hardware

| Card | Microarchitecture | Compute Capability | Result |
|---|---|---|---|
| **GTX 970** | Maxwell | 5.2 | ✗ **Fails** |
| **GTX 1650** | Turing | 7.5 | ✓ Works |
| **RTX 3060** | Ampere | 8.6 | ✓ Works |

These match the official rule exactly: the two working cards are Turing /
Ampere (both in the supported microarchitecture set); the failing card is
Maxwell (not in the set).

### Practical guidance

- **Turing / Ampere / Ada Lovelace / Hopper** — supported; consumer cards on
  these architectures (RTX 20xx/30xx/40xx/50xx, GTX 16xx, T4/A10/A100/L4/H100)
  work. Confirmed on Turing and Ampere.
- **Same-microarchitecture consumer cards** — officially "likely work" (e.g.
  RTX 3090, RTX 4090); not officially supported, so an incompatible workload on
  one should be reported to the gVisor project.
- **Maxwell (GTX 900) and older** — not in the supported set → fails (tested:
  GTX 970).
- **Pascal (GTX 10 series, e.g. GTX 1080)** — also **not** in the supported set
  (only Turing and newer are). Expect the same failure as Maxwell; verify before
  committing hardware.
- **Edge / embedded / special GPUs** — Jetson / Tegra, laptop "MX" cards,
  integrated graphics — **uncertain**: they don't follow the same desktop
  driver path and are not covered by the tests or the official list. Verify
  per-device.

**Failure signature:** the sandbox proxies to the *host's* driver, and the
supported driver + CUDA container stacks (the example below uses CUDA 13) do
not cover Maxwell/Pascal-class compute. The driver installs, `nvidia-smi` runs,
but CUDA compute fails — so the card can't be used under NVProxy.

---

## 1. Install runsc

```bash
sudo apt-get update && \
sudo apt-get install -y \
    apt-transport-https \
    ca-certificates \
    curl \
    gnupg

curl -fsSL https://gvisor.dev/archive.key | sudo gpg --dearmor -o /usr/share/keyrings/gvisor-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/gvisor-archive-keyring.gpg] https://storage.googleapis.com/gvisor/releases release main" | sudo tee /etc/apt/sources.list.d/gvisor.list > /dev/null

sudo apt-get update && sudo apt-get install -y runsc
```

## 2. Register the runtime (CPU-only, no GPU)

Create/update `/etc/docker/daemon.json`:

```json
{
  "runtimes": {
    "runsc": {
      "path": "/usr/bin/runsc"
    }
  }
}
```

Then `sudo systemctl restart docker`. You can now run CPU-only sandboxes with
`docker run --runtime runsc ...`.

## 3. NVIDIA GPU passthrough

### 3.1 Pick a driver version NVProxy supports

```bash
runsc nvproxy list-supported-drivers
```

The current list includes (among others):

```
535.129.03  535.183.06  535.247.01  535.261.03  535.274.02  535.288.01  535.309.01
550.90.12
570.124.06  570.133.20  570.172.08  570.195.03
580.65.06   580.105.08  580.126.09  580.126.20  580.159.03  580.159.04  580.173.02
590.48.01
615.15.00
620.06.00
```

**You must use one of these exact versions** — NVProxy validates the driver and
rejects mismatched versions. Use the newest one that still supports your GPU's
architecture (Maxwell-class cards are dropped from the newer branches, see the
compatibility table above).

### 3.2 Install (or upgrade to) that driver

```bash
# Example: 580.173.02
wget -c https://us.download.nvidia.com/XFree86/Linux-x86_64/580.173.02/NVIDIA-Linux-x86_64-580.173.02.run

# Fully remove any existing NVIDIA packages + nouveau
sudo apt purge -y "*nvidia*" "*libnvidia*"
sudo apt autoremove -y --purge
sudo apt clean

# Blacklist nouveau
sudo tee /etc/modprobe.d/blacklist-nouveau.conf << EOF
blacklist nouveau
options nouveau modeset=0
EOF
sudo update-initramfs -u

sudo apt update
sudo apt install -y build-essential dkms linux-headers-$(uname -r)

# Reboot is required now; the boot may drop to a plain terminal
# sudo reboot
```

After reboot, run the installer:

```bash
chmod +x NVIDIA-Linux-x86_64-580.173.02.run
sudo ./NVIDIA-Linux-x86_64-580.173.02.run
```

### 3.3 (Re)install the NVIDIA Container Toolkit

```bash
sudo apt-get update && sudo apt-get install -y --no-install-recommends \
   ca-certificates \
   curl \
   gnupg2

curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey | sudo gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg \
  && curl -s -L https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list | \
    sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' | \
    sudo tee /etc/apt/sources.list.d/nvidia-container-toolkit.list

sudo apt-get update
export NVIDIA_CONTAINER_TOOLKIT_VERSION=1.19.1-1
sudo apt-get install -y \
    nvidia-container-toolkit=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
    nvidia-container-toolkit-base=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
    libnvidia-container-tools=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
    libnvidia-container1=${NVIDIA_CONTAINER_TOOLKIT_VERSION}
```

### 3.4 Enable NVProxy in the Docker daemon

Update `/etc/docker/daemon.json` to pass `--nvproxy` to runsc:

```json
{
  "runtimes": {
    "runsc": {
      "path": "/usr/bin/runsc",
      "runtimeArgs": [
        "--nvproxy"
      ]
    }
  }
}
```

### 3.5 Restart Docker

```bash
sudo systemctl restart docker
```

### 3.6 Verify GPU works under the sandbox

```bash
docker run --rm --gpus all --runtime runsc nvidia/cuda:13.0.1-base-ubuntu22.04 nvidia-smi
```

The container must print the host GPU via NVProxy:

```
Thu Jul 30 05:49:27 2026
+-----------------------------------------------------------------------------------------+
| NVIDIA-SMI 580.173.02             Driver Version: 580.173.02     CUDA Version: 13.0     |
+-----------------------------------------+------------------------+----------------------+
| GPU  Name                 Persistence-M | Bus-Id          Disp.A | Volatile Uncorr. ECC |
| Fan  Temp   Perf          Pwr:Usage/Cap |           Memory-Usage | GPU-Util  Compute M. |
|                                         |                        |               MIG M. |
|=========================================+========================+======================|
|   0  NVIDIA GeForce RTX 3050        Off |   00000000:01:00.0 Off |                  N/A |
| 30%   30C    P8              7W /   70W |     169MiB /   6144MiB |      0%      Default |
|                                         |                        |                  N/A |
+-----------------------------------------+------------------------+----------------------+
```

If `nvidia-smi` runs but **CUDA compute** fails, the GPU's architecture is
likely not supported by this driver/CUDA stack (see the compatibility table).

## 4. Using it in OpenWorkspace

Once the runtime works with `docker run --runtime runsc`, set the Template's
**Container Runtime** to `gVisor` in the dashboard. The API launches that
template's instances with the `runsc` runtime. CPU-only templates work the same
way; GPU templates additionally need a CUDA-compatible image (e.g. a `cuda`
base image) and a supported host driver.
