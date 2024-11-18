#!/bin/sh

fn=".10404421-a9ab-4062-a911-a893857f35fd"

{
  test -f "$fn" ||
  {
    touch "$fn"
    false
  }
} &&
{
  rm "$fn"
}
