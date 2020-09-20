package main

import (
	"fmt"
	"os"
)

func dieNoRoomID() {
	fmt.Println("no operation supplied")
	os.Exit(1)
}
