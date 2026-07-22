inner() {
  echo 123 | grep 123
}
outer="$(inner)"
printf 'outer=<%s>\n' "$outer"
