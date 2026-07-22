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
# 3. 安裝 Docker CLI
# =========================
RUN apt-get update && \
    apt-get install -y ca-certificates curl gnupg

RUN install -m 0755 -d /etc/apt/keyrings

RUN curl -fsSL --retry 3 --retry-delay 2 https://download.docker.com/linux/ubuntu/gpg \
    | gpg --dearmor -o /etc/apt/keyrings/docker.gpg

RUN chmod a+r /etc/apt/keyrings/docker.gpg

RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu noble stable" \
    | tee /etc/apt/sources.list.d/docker.list > /dev/null

RUN apt-get update && \
    apt-get install -y docker-ce-cli docker-buildx-plugin docker-compose-plugin

# 建立 docker group 並加入 user
RUN groupadd -f docker && \
    usermod -aG docker ${USERNAME}

# =========================
# 4. 切換使用者
# =========================
USER ${USERNAME}
WORKDIR /home/${USERNAME}

# =========================
# 5. 安裝 uv
# =========================
RUN curl -LsSf https://astral.sh/uv/install.sh | sh
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# =========================
# 6. 安裝 nvm + node
# =========================
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.2/install.sh | bash && \
    . "$NVM_DIR/nvm.sh" && \
    nvm install 24 && \
    nvm alias default 24 && \
    nvm use default

# =========================
# 7. PATH 設定
# =========================
RUN echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc && \
    echo '[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"' >> ~/.bashrc

# =========================
# 8. workspace
# =========================
RUN mkdir -p /home/${USERNAME}/workspace/CLIvsMCP

# =========================
# 9. tools install
# =========================
RUN curl -fsSL https://opencode.ai/install | bash
RUN curl -fsSL https://raw.githubusercontent.com/colbymchenry/codegraph/main/install.sh | sh

ENV PATH="/home/${USERNAME}/.local/bin:/usr/local/bin:${PATH}"

WORKDIR /home/${USERNAME}/workspace/CLIvsMCP

# codegraph install --yes

CMD ["/bin/bash"]