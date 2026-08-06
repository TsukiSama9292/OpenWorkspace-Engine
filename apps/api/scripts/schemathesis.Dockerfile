# Schemathesis image for the apps/api security fuzzing harness (02-be).
#
# The official schemathesis/schemathesis:stable image (4.24.3) bundles an
# incompatible `tracecov` report plugin that crashes `schemathesis run` with
# `KeyError: 'coverage_format'` on every invocation, so this project builds
# its own image from PyPI instead — no host Python / pipx required.
FROM python:3.12-slim
RUN pip install --no-cache-dir --root-user-action=ignore schemathesis==4.24.3
WORKDIR /work
