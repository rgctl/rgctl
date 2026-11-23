operand_sink() { printf "%s" "$1"; }
operand_relay() { local alias_value="$1"; operand_sink "$alias_value"; }
