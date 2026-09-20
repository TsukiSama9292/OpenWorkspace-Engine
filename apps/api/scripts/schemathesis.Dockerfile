# Schemathesis image for the apps/api security fuzzing harness (02-be).
#
# The official schemathesis/schemathesis:stable image (4.24.3) bundles an
# incompatible `tracecov` report plugin that crashes `schemathesis run` with
# `KeyError: 'coverage_format'` on every invocation, so this project builds
# its own image from PyPI instead — no host Python / pipx required.
FROM python:3.12-slim
# schemathesis==4.24.3 is pinned, but its transitive deps are not: a fresh
# build pulls the latest jsonschema-rs (0.56.0), whose CanonicalSchema no
# longer carries the `is_satisfiable` schemathesis 4.24.3 calls when
# generating `{id}` path params — AttributeError on every {id} endpoint
# (5 runtime errors, zero product failures). Pin hypothesis + jsonschema-rs
# to the milestone-green era (2026-08).
RUN pip install --no-cache-dir --root-user-action=ignore \
    schemathesis==4.24.3 \
    hypothesis==6.165.2 \
    jsonschema-rs==0.49.9
WORKDIR /work
