#!/bin/sh
set -eu

config_path=${FIXER_CONFIG:-/data/fixer.toml}
config_template=${FIXER_CONFIG_TEMPLATE:-/usr/share/fixer/fixer.toml.example}
if [ ! -e "$config_path" ]; then
  case "$config_path" in
  */*) config_dir=${config_path%/*} ;;
  *) config_dir=. ;;
  esac
  mkdir -p "$config_dir"
  umask 077
  cp "$config_template" "$config_path"
fi

exec "$@"
