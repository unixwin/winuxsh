true && echo yes || echo no
false && echo bad1 || echo fallback
true || echo bad2
false || echo recovered
false && echo bad3 && echo also_bad
true || echo bad4 && echo after_true
[ 1 -eq 1 ] && echo test_true || echo test_false
[ 1 -eq 2 ] && echo bad5 || echo test_false
false
echo status:$?
true
echo status:$?
