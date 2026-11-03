import Foundation

typealias UserId = Int

protocol Named {
  associatedtype ID
  var title: String { get }
  func label() -> String
}

enum Color {
  case red
  case green
}

class Base {
  init() {}
  deinit {}
  subscript(i: Int) -> Int { return i }
}

class Widget: Named {
  typealias ID = Int
  var title: String = "w"
  var count: Int = 0

  func label() -> String { return title }

  func run(items: [Int]) async throws -> Int {
    var i = 0
    i = i + 1
    let ok = i > 0 && i < 10 || i == 0
    let ranged = 0..<10
    let cast = i as Int
    let tern = ok ? 1 : 0
    let nav = self.title
    let closure = { (n: Int) -> Int in n + 1 }
    _ = closure(1)
    for item in items {
      if item > 0 {
        break
      } else {
        continue
      }
    }
    while i < 5 {
      i = i + 1
    }
    repeat {
      i = i + 1
    } while i < 8
    guard i > 0 else { return 0 }
    switch i {
    case 1:
      break
    default:
      break
    }
    do {
      try await Task.sleep(nanoseconds: 1)
    } catch {
      throw error
    }
    return tern + cast.hashValue + ranged.count + nav.count
  }
}
