package main

import (
	"errors"
	"math/rand"
)

// ErrListExhausted means a stratum ran out of pre-generated blocks; enrolment must stop, not improvise.
var ErrListExhausted = errors.New("allocation list exhausted for stratum")

// Scheme is the randomisation design from the protocol.
type Scheme struct {
	Arms      []string       `json:"arms"`
	Strata    []string       `json:"strata"`
	BlockSize int            `json:"blockSize"`
	Ratio     map[string]int `json:"ratio"`
}

// BlockList is the allocation sequence for one stratum, consumed strictly in order.
type BlockList struct {
	Stratum string
	Slots   []string
	Next    int
}

// GenerateBlocks builds permuted blocks honouring the allocation ratio, e.g. 2:1 in blocks of 6.
func GenerateBlocks(s Scheme, stratum string, blocks int, rng *rand.Rand) (*BlockList, error) {
	perBlock := 0
	for _, n := range s.Ratio {
		perBlock += n
	}
	if perBlock == 0 || s.BlockSize%perBlock != 0 {
		return nil, errors.New("block size must be a multiple of the allocation ratio")
	}
	list := &BlockList{Stratum: stratum}
	for b := 0; b < blocks; b++ {
		block := make([]string, 0, s.BlockSize)
		for _, arm := range s.Arms {
			for i := 0; i < s.Ratio[arm]*s.BlockSize/perBlock; i++ {
				block = append(block, arm)
			}
		}
		rng.Shuffle(len(block), func(i, j int) { block[i], block[j] = block[j], block[i] })
		list.Slots = append(list.Slots, block...)
	}
	return list, nil
}

// Take allocates the next slot of the list.
func (l *BlockList) Take() (string, error) {
	if l.Next >= len(l.Slots) {
		return "", ErrListExhausted
	}
	arm := l.Slots[l.Next]
	l.Next++
	return arm, nil
}

// StratumKey joins the subject's stratification factors in the order the protocol lists them.
func StratumKey(s Scheme, factors map[string]string) (string, error) {
	key := ""
	for _, f := range s.Strata {
		v, ok := factors[f]
		if !ok {
			return "", errors.New("missing stratification factor " + f)
		}
		key += f + "=" + v + ";"
	}
	return key, nil
}
