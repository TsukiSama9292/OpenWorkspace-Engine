# gVisor

## 安裝
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

## 編輯或建立 Docker 的 daemon 設定檔 (/etc/docker/daemon.json)
```JSON
{
  "runtimes": {
    "runsc": {
      "path": "/usr/bin/runsc"
    }
  }
}
```

## 使用 Nvidia GPU

> 建議使用 RTX 以上的系列

### gVisor 要用 GPU 必須要它自己有支援的版本
```bash
$ runsc nvproxy list-supported-drivers
535.129.03
535.183.06
535.247.01
535.261.03
535.274.02
535.288.01
535.309.01
550.90.12
570.124.06
570.133.20
570.172.08
570.195.03
580.65.06
580.105.08
580.126.09
580.126.20
580.159.03
580.159.04
580.173.02
590.48.01
615.15.00
620.06.00
```

### 下載有支援的驅動版本

```bash
wget -c https://us.download.nvidia.com/XFree86/Linux-x86_64/580.173.02/NVIDIA-Linux-x86_64-580.173.02.run
sudo apt purge -y "*nvidia*" "*libnvidia*"
sudo apt autoremove -y --purge
sudo apt clean
sudo tee /etc/modprobe.d/blacklist-nouveau.conf << EOF
blacklist nouveau
options nouveau modeset=0
EOF
sudo update-initramfs -u
sudo apt update
sudo apt install -y build-essential dkms linux-headers-$(uname -r)
### 此處需要重啟, 啟動時可能只有終端機
# sudo reboot
```

### 授權執行並安裝驅動

```bash
chmod +x NVIDIA-Linux-x86_64-580.173.02.run
sudo ./NVIDIA-Linux-x86_64-580.173.02.run
```

### 重新安裝 nvidia container toolkit

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

### 編輯或建立 Docker 的 daemon 設定檔 (/etc/docker/daemon.json)

```JSON
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
### 重啟 docker 服務

```bash
sudo systemctl restart docker
```

### 驗證

```bash
$ docker run --rm --gpus all --runtime runsc nvidia/cuda:13.0.1-base-ubuntu22.04 nvidia-smi
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

+-----------------------------------------------------------------------------------------+
| Processes:                                                                              |
|  GPU   GI   CI              PID   Type   Process name                        GPU Memory |
|        ID   ID                                                               Usage      |
|=========================================================================================|
|  No running processes found                                                             |
+-----------------------------------------------------------------------------------------+
```

{
    "runtimes": {
        "nvidia": {
            "args": [],
            "path": "nvidia-container-runtime"
        },
        "runsc": {
            "path": "/usr/bin/runsc"
        }
    }
}
