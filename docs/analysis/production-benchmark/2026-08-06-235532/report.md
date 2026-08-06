# Production stack CPU/RAM benchmark

- Timestamp: 2026-08-07T00:03:29+08:00
- Windows: 60 s each (1 sample/s)
- Docker default runtime: runc
- Compose file: docker/openworkspace/docker-compose.yml @ 275bcf3
- CPU: 12th Gen Intel(R) Core(TM) i5-12400F (12 thread(s)) @ up to 4400 MHz
- RAM: 32 GB (DDR4-2666, 2 module(s))
- Platform: ow-traefik ow-postgres ow-web ow-api
- Instances: 3 remote types x runc/runsc, dini, no_persistent
- Template images:
  - tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy
  - tsukisama9292/ow-ttyd-ubuntu-dini:jammy
  - tsukisama9292/ow-jupyter-ubuntu-dini:jammy

## 1. Platform container peaks

| container | peak_cpu | peak_mem |
| --- | --- | --- |
| ow-traefik | 0.00 | 18958254 |
| ow-postgres | 3.66 | 35378954 |
| ow-web | 3.10 | 10884219 |
| ow-api | 3.42 | 2821718 |

## 2. Per-instance peaks

| instance | remote_type | runtime | peak_cpu | peak_mem |
| --- | --- | --- | --- | --- |
| bench-runsc-kasmvnc-1 | kasmvnc | runsc | 207.77 | 909534822 |
| bench-runc-kasmvnc-1 | kasmvnc | runc | 4.87 | 320444826 |
| bench-runsc-ttyd-1 | ttyd | runsc | 4.94 | 96301220 |
| bench-runc-ttyd-1 | ttyd | runc | 2.44 | 45267026 |
| bench-runsc-jupyter-1 | jupyter | runsc | 1.76 | 223241830 |
| bench-runc-jupyter-1 | jupyter | runc | 0.24 | 172595610 |

## 3. runC vs runsc aggregate (per remote type)

| runtime | remote_type | mean_cpu | peak_cpu | mean_mem | peak_mem |
| --- | --- | --- | --- | --- | --- |
| runsc | kasmvnc | 13.58 | 207.77 | 904332138 | 909534822 |
| runc | kasmvnc | 1.26 | 4.87 | 319803447 | 320444826 |
| runsc | ttyd | 0.96 | 4.94 | 95829710 | 96301220 |
| runc | ttyd | 0.15 | 2.44 | 45192053 | 45267026 |
| runsc | jupyter | 0.88 | 1.76 | 222764728 | 223241830 |
| runc | jupyter | 0.11 | 0.24 | 172476771 | 172595610 |

## 4. Host before -> after

| metric | before | after | delta |
| --- | --- | --- | --- |
| cpu_percent | 2.65 | 5.14 | 2.49 |
| mem_available_bytes | 25003656670 | 23111265348 | -1892391322 |

