// Identical source in different modules also exercises content-keyed facts reuse.
function moduleHelper() { return 1; }
function moduleExercise() { return moduleHelper(); }
function moduleOuter() {
  function moduleInner() { return 2; }
  return moduleInner();
}
