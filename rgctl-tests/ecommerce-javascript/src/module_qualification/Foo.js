class Foo {
  static create() { return 1; }
  method() {
    function nativeCallback() { return 3; }
    return nativeCallback();
  }
}
function create() {
  function collisionInner() { return 2; }
  return collisionInner();
}
function collisionCaller() { return create(); }
