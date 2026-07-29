```bash
docker build -t tsukisama9292/ow-jupyter-ubuntu:jammy -f Dockerfile.jupyterlab_ubuntu .
docker build -t tsukisama9292/ow-ttyd-ubuntu:jammy -f Dockerfile.ttyd_ubuntu .
docker build -t tsukisama9292/ow-kasmvnc-ubuntu:jammy -f Dockerfile.kasmvnc_ubuntu --build-arg BASE_TAG=1.19.0-rolling-daily .
```

```bash
docker run -t -d -p 8888:8888 tsukisama9292/ow-jupyter-ubuntu:jammy
docker run -t -d -p 7681:7681 tsukisama9292/ow-ttyd-ubuntu:jammy
docker run -t -d -p 6901:6901 -e VNC_PW=password tsukisama9292/ow-kasmvnc-ubuntu:jammy
```