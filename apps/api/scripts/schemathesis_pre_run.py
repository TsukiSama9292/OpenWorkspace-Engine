"""Custom Schemathesis checks for the security fuzzing harness (02-be).

Loaded via `schemathesis.toml` (`hooks = "schemathesis_pre_run"`) and mounted
into the container read-only; the module resolves through `PYTHONPATH=/harness`.

Custom checks registered here run on EVERY invocation regardless of the
`-c/--checks` flag (verified against Schemathesis 4.24.3), so activation is
gated on the `OW_ENFORCE_RBAC` environment variable instead of check lists:
Pass 1 (admin session) leaves it unset and the assertions are inert; Pass 2
(fuzz-user session) sets it to `1` and the RBAC boundary is enforced.
"""

import os

import schemathesis


@schemathesis.check
def admin_gated_boundary(context, response, case):
    """admin-gated endpoints must never answer 2xx to a non-admin session.

    Pass 2 runs the whole 20-endpoint surface under the low-privilege
    fuzz-user's cookie. Any operation tagged `admin-gated` returning a 2xx
    status is a boundary violation; 403 (or 404) are the accepted outcomes.
    """
    if os.environ.get("OW_ENFORCE_RBAC") != "1":
        return

    tags = list(getattr(case.operation, "tags", None) or [])
    if "admin-gated" not in tags:
        return

    status = response.status_code
    if status < 400:
        raise AssertionError(
            f"{case.method} {case.path} is admin-gated but returned "
            f"{status} to the non-admin session"
        )
