#!/bin/bash
set -e
set -x

wget https://dbeaver.io/files/dbeaver-ce_latest_amd64.deb && \
    dpkg -i dbeaver-ce_latest_amd64.deb || true && \
    apt-get install -f -y && \
    rm -f dbeaver-ce_latest_amd64.deb