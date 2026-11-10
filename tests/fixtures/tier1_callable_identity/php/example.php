<?php
class Base { function quote(int $value): int { return 11; } }
class Child extends Base { function quote(int $value): int { return 21; } }
