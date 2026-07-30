# ow_user is allowed to run apt commands without a password prompt

```
echo "ow_user ALL=(ALL) NOPASSWD: /usr/bin/apt, /usr/bin/apt-get" > /etc/sudoers.d/ow_user && chmod 0440 /etc/sudoers.d/ow_user
```