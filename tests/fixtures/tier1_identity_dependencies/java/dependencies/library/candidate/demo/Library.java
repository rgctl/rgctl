package demo;

public class Library {
    public static int quote(int value) {
        return value + 1;
    }

    public static int quote(String value) {
        return value.length() + 1;
    }
}
