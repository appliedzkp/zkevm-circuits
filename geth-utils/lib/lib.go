package main

/*
   #include <stdlib.h>
*/
import "C"
import (
	"encoding/json"
	"fmt"
	"main/gethutil"
	"main/gethutil/mpt/witness"
	"unsafe"
)

// TODO: Add proper error handling.  For example, return an int, where 0 means
// ok, and !=0 means error.
//
//export CreateTrace
func CreateTrace(configStr *C.char) *C.char {
	var config gethutil.TraceConfig
	err := json.Unmarshal([]byte(C.GoString(configStr)), &config)
	if err != nil {
		return C.CString(fmt.Sprintf("Failed to unmarshal config, err: %v", err))
	}

	executionResults, err := gethutil.Trace(config)
	if err != nil {
		return C.CString(fmt.Sprintf("Failed to run Trace, err: %v", err))
	}

	bytes, err := json.MarshalIndent(executionResults, "", "  ")
	if err != nil {
		return C.CString(fmt.Sprintf("Failed to marshal []ExecutionResult, err: %v", err))
	}

	return C.CString(string(bytes))
}

type Config struct {
	NodeUrl  string   `json:"NodeUrl"`
	BlockNum int      `json:"BlockNum"`
	Addr     string   `json:"Addr"`
	Keys     []string `json:"Keys"`
	Values   []string `json:"Values"`
}

type GetWitnessRequest struct {
	BlockNum int    `json:"BlockNum"`
	NodeUrl  string `json:"NodeUrl"`
	Mods     []witness.TrieModification
}

//export GetMptWitness
func GetMptWitness(proofConf *C.char) *C.char {
	var config GetWitnessRequest

	err := json.Unmarshal([]byte(C.GoString(proofConf)), &config)
	if err != nil {
		panic(err)
	}

	proof := witness.GetWitness(config.NodeUrl, config.BlockNum, config.Mods)
	b, err := json.Marshal(proof)
	if err != nil {
		fmt.Println(err)
	}

	return C.CString(string(b))
}

//export FreeString
func FreeString(str *C.char) {
	C.free(unsafe.Pointer(str))
}

func main() {}
