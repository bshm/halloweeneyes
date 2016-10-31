#!/bin/bash

find . -name "*.cpp" -o -name "*.h" | \
  grep -v '^./libs' | \
  uncrustify --no-backup -c uncrustify.cfg -F /dev/stdin
