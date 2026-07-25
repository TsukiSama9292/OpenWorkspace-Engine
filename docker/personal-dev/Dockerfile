# 使用 Ubuntu 24.04 作為基底
FROM ubuntu:24.04

# =========================
# 🔧 Build Args（可外部傳入）
# =========================
ARG USERNAME=user
ARG USER_UID=1000
ARG USER_GID=1000

# 避免安裝過程中的互動式提示
ENV DEBIAN_FRONTEND=noninteractive

# 設定 nvm 與 uv 的路徑
ENV NVM_DIR=/home/${USERNAME}/.nvm
ENV UV_INSTALL_DIR=/home/${USERNAME}/.local/bin

# =========================
# 1. 基礎套件安裝
# =========================
RUN apt-get update && apt-get upgrade -y && \
    apt-get install -y \
        sudo \
        curl \
        wget \
        git \
        python3 \
        python3-pip \
        ca-certificates \
        lsb-release \
        gnupg \
        postgresql-client \
        libssl-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# =========================
# 2. 建立 USER / GROUP（完全 ARG 化）
# =========================
RUN if getent passwd ${USER_UID}; then \
        userdel -r $(getent passwd ${USER_UID} | cut -d: -f1); \
    fi && \
    if getent group ${USER_GID}; then \
        groupdel $(getent group ${USER_GID} | cut -d: -f1); \
    fi && \
    groupadd -g ${USER_GID} ${USERNAME} && \
    useradd -m -u ${USER_UID} -g ${USER_GID} -s /bin/bash ${USERNAME} && \
    echo "${USERNAME} ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

# =========================
# 3. 安裝 Docker Engine (DinD 所需的 dockerd)
# =========================
RUN install -m 0755 -d /etc/apt/keyrings

RUN curl -fsSL --retry 3 --retry-delay 2 https://download.docker.com/linux/ubuntu/gpg \
    | gpg --dearmor -o /etc/apt/keyrings/docker.gpg

RUN chmod a+r /etc/apt/keyrings/docker.gpg

RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu noble stable" \
    | tee /etc/apt/sources.list.d/docker.list > /dev/null

# 注意：這裡除了 cli，更重要是安裝 docker-ce (包含 dockerd) 與 containerd.io
RUN apt-get update && \
    apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin && \
    rm -rf /var/lib/apt/lists/*

# 建立 docker group 並將 user 加入
RUN groupadd -f docker && \
    usermod -aG docker ${USERNAME}

# =========================
# 4. 切換使用者
# =========================
USER ${USERNAME}
WORKDIR /home/${USERNAME}

# =========================
# 5. 安裝 Cargo + Rust
# =========================
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/home/${USERNAME}/.cargo/bin:${PATH}"
RUN cargo install cargo-llvm-cov --version 0.8.7 --locked
# =========================
# 6. 安裝 nvm + node 並綁定系統全域路徑
# =========================
ENV NVM_DIR="/home/${USERNAME}/.nvm"

RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | bash && \
    export NVM_DIR="$HOME/.nvm" && \
    [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh" && \
    nvm install 24 && \
    nvm alias default 24 && \
    NODE_BIN="$(dirname $(nvm which default))" && \
    sudo ln -sf "$NODE_BIN"/* /usr/local/bin/
# =========================
# 7. PATH & bashrc 設定
# =========================
# 補充使用者個人的 local/bin 到 PATH 中
ENV PATH="/home/${USERNAME}/.local/bin:${PATH}"

RUN echo '[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"' >> ~/.bashrc && \
    echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
# =========================
# 8. workspace
# =========================
RUN mkdir -p /home/${USERNAME}/workspace/OpenWorkspace-Engine
WORKDIR /home/${USERNAME}/workspace/OpenWorkspace-Engine

# =========================
# 9. tools install
# =========================
RUN curl -fsSL https://opencode.ai/install | bash
RUN curl -fsSL https://raw.githubusercontent.com/colbymchenry/codegraph/main/install.sh | sh
RUN wget -qO- https://get.pnpm.io/install.sh | ENV="$HOME/.bashrc" SHELL="$(which bash)" bash -
ENV PATH="/home/${USERNAME}/.local/bin:/usr/local/bin:${PATH}"
RUN rustup component add llvm-tools-preview
RUN cargo install cargo-llvm-cov --version 0.8.7 --locked
RUN cargo install cargo-nextest --version 0.9.140 --locked
RUN cargo install rustfilt --version 0.2.1 --locked
# codegraph install --yes

# =========================
# 建立 DinD Entrypoint 腳本
# =========================
USER root
RUN echo '#!/bin/bash\n\
# 1. 啟動背景 dockerd (強制指定 vfs 驅動以避開 overlay-on-overlay 限制)\n\
#    --cgroup-parent=/ 避免 threaded cgroup 層級問題\n\
sudo dockerd --storage-driver=vfs --cgroup-parent=/ --exec-opt native.cgroupdriver=cgroupfs > /tmp/dockerd.log 2>&1 &\n\
\n\
# 2. 使用 sudo 檢查 dockerd 是否啟動完成（避免一般使用者權限問題）\n\
TRY=0\n\
while ! sudo docker info >/dev/null 2>&1; do\n\
    TRY=$((TRY+1))\n\
    if [ $TRY -gt 15 ]; then\n\
        echo "❌ Docker daemon 啟動失敗！日誌如下：" \n\
        cat /tmp/dockerd.log\n\
        exit 1\n\
    fi\n\
    sleep 1\n\
done\n\
\n\
# 3. 確定起來後，立即開放權限給普通使用者\n\
sudo chmod 666 /var/run/docker.sock\n\
echo "✅ Docker daemon 啟動成功！"\n\
\n\
exec "$@"' > /entrypoint.sh && chmod +x /entrypoint.sh

USER ${USERNAME}

ENTRYPOINT ["/entrypoint.sh"]
CMD ["/bin/bash"]