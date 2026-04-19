/* vtable-corruption.cpp — C++ vtable pointer corruption via memset.
 *
 * Showcases: C++ debugging with corrupted virtual dispatch.  A
 * polymorphic object's vtable pointer is zeroed with memset, then a
 * virtual method call crashes on the corrupted dispatch.  Exercises
 * C++ name demangling and memory inspection of object layout.
 *
 * Compile: g++ -g -O0 -o /tmp/vtable-corruption examples/programs/vtable-corruption.cpp
 *
 * Scheme session — inspect the corrupted object:
 *   (begin
 *     (load-file "/tmp/vtable-corruption")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV from corrupted vtable
 *     (backtrace)               ;; shows the virtual dispatch failure
 *     ;; Inspect the object's memory to see the zeroed vtable pointer
 *     (inspect "shape")
 *     (inspect "sizeof(*shape)")
 *     (mi "-data-read-memory-bytes &(*shape) 32"))
 */

#include <cstdio>
#include <cstring>

class Shape {
public:
    virtual ~Shape() = default;
    virtual double area() const = 0;
    virtual const char *name() const = 0;
};

class Circle : public Shape {
public:
    Circle(double r) : radius(r) {}

    double area() const override {
        return 3.14159265358979323846 * radius * radius;
    }

    const char *name() const override {
        return "Circle";
    }

private:
    double radius;
};

class Rectangle : public Shape {
public:
    Rectangle(double w, double h) : width(w), height(h) {}

    double area() const override {
        return width * height;
    }

    const char *name() const override {
        return "Rectangle";
    }

private:
    double width;
    double height;
};

void print_shape(Shape *shape) {
    /* This will crash — the vtable pointer has been zeroed. */
    printf("%s: area = %.2f\n", shape->name(), shape->area());
}

int main() {
    Circle *circle = new Circle(5.0);
    printf("before corruption: %s area = %.2f\n",
           circle->name(), circle->area());

    /* Corrupt the vtable pointer by zeroing the object's memory.
     * The vtable pointer is typically the first 8 bytes of the object. */
    Shape *shape = circle;
    printf("corrupting vtable at %p\n", static_cast<void *>(shape));
    std::memset(shape, 0, sizeof(void *));  /* Zero just the vtable pointer. */

    /* Virtual dispatch now reads address 0x0 for the vtable,
     * then tries to read a function pointer from that address — SIGSEGV. */
    print_shape(shape);

    delete circle;
    return 0;
}
