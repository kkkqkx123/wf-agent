CC ?= cc
CFLAGS ?= -O2 -fPIC -Wall -Wextra
TARGET ?= native_echo.so

all: $(TARGET)

$(TARGET): plugin.c
	$(CC) $(CFLAGS) -shared -o $@ plugin.c

clean:
	rm -f $(TARGET)
