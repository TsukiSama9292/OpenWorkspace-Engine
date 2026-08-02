```bash
# Build all template images (regular + *_dini) in dependency order
bash build.sh
```

```bash
docker run -t -d -p 8888:8888 tsukisama9292/ow-jupyter-ubuntu:jammy
docker run -t -d -p 7681:7681 tsukisama9292/ow-ttyd-ubuntu:jammy
docker run -t -d -p 6901:6901 -e VNC_PW=password tsukisama9292/ow-kasmvnc-ubuntu:jammy
```

The `*_dini` variants add an in-instance Docker daemon. They behave exactly
like the regular images unless `OW_DOCKER_IN_INSTANCE=true` is set, in which
case the entrypoint starts `dockerd --iptables=false --ip6tables=false
--data-root=/var/lib/docker`, waits up to 15 s for `docker info` readiness
(logs and exits non-zero on failure), then starts the main service. The API
provisions these containers as `privileged` with a `/var/lib/docker` tmpfs.

```bash
docker run -t -d -p 7681:7681 \
  --privileged --tmpfs /var/lib/docker:exec,mode=755 \
  -e OW_DOCKER_IN_INSTANCE=true \
  tsukisama9292/ow-ttyd-ubuntu-dini:jammy
```

```bash
docker push tsukisama9292/ow-jupyter-ubuntu:jammy
docker push tsukisama9292/ow-ttyd-ubuntu:jammy
docker push tsukisama9292/ow-kasmvnc-ubuntu:jammy
docker push tsukisama9292/ow-jupyter-ubuntu-dini:jammy
docker push tsukisama9292/ow-ttyd-ubuntu-dini:jammy
docker push tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy
```