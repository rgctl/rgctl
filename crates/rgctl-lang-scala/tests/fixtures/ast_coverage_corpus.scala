// Line comment for coverage
/* block comment */
package demo.corp

import scala.collection.mutable.{Map => MMap, ListBuffer => _}
export demo.corp.{Widget as W}

given intOrd: Ordering[Int] = Ordering.Int

enum Color derives CanEqual {
  case Red, Green
  case Blue(v: Int)
}

trait Named {
  def label: String
  val title: String
  var flag: Boolean
}

extension (s: String) {
  def shout: String = s.toUpperCase
}

object Helper {
  type Id = Int
  def idOf(xs: Seq[Int]): Id = xs.sum
  def idOf(x: Int): Id = x
}

package object pkgmeta {
  val version = 1
}

@main
class Widget[+A <: CharSequence, -B >: Null, C: Ordering](
  val x: Int,
  var y: Int = 0,
  extras: String*
) extends Named {
  self: Named =>

  def label: String = "w"
  val title: String = "t"
  var flag: Boolean = false

  private val items = List(1, 2, 3)
  private var count = 0

  def run(xs: List[Int])(using Ordering[Int]): Int = {
    val (a, b) = (1, 2)
    val tup = (1, "x", true)
    val f: Int => Int = (n: Int) => n + 1
    val g = { (n: Int) => n * 2 }
    count = a + b
    val ascr: Any = count: Int
    val pref = -count
    val headVal = items.head
    val field = this.x
    val call = Math.max(1, 2)
    val infix = 1 + 2
    val gen = List[Int](1, 2)
    val inst = new String("hi")
    val interp = s"count=$count"
    val namedTup: (a: Int, b: String) = (a = 1, b = "x")
    val post = xs toList
    Helper.idOf(xs: _*)
    xs.map: n =>
      n + 1
    for {
      i <- xs if i > 0
      j <- List(i)
    } yield i + j
    xs match {
      case Nil | List() => 0
      case h :: t if h > 0 => h
      case List(p, q) => p + q
      case Foo(n) => n
      case Foo(n = v) => v
      case named @ Bar(v) => v
      case s: String => s.length
      case (p, q) => p
      case _ => -1
    }
    try {
      if (count < 5) while (count < 3) { count += 1 }
      do { count += 1 } while (count < 4)
      throw new RuntimeException("boom")
    } catch {
      case e: RuntimeException => e.getMessage
    } finally {
      println("done")
    }
    return f(1)
  }
}

case class Foo(n: Int)
case class Bar(v: Int)

abstract class Abs {
  def foo: Int
  val bar: String
  var baz: Int
}

using path = "."
