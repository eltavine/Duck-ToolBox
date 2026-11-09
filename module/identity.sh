# shellcheck shell=sh

duck_module_id() (
  id=$(sed -n 's/^id=//p' "$1/module.prop") || exit 1
  case "$id" in duck-toolbox|duck-toolbox-lite) ;; *) exit 1 ;; esac
  case "${1##*/}" in
    duck-toolbox|duck-toolbox-lite) [ "${1##*/}" = "$id" ] || exit 1 ;;
  esac
  printf '%s\n' "$id"
)

conflicting_installation() (
  case "$1" in
    duck-toolbox-lite) other=duck-toolbox ;;
    *) other=duck-toolbox-lite ;;
  esac
  [ -d "$2/modules/$other" ] || [ -d "$2/modules_update/$other" ]
)
