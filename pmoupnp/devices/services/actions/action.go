package actions

import "maps"

type Action struct {
	name      string
	arguments ArgumentSet
}

func NewAction(name string) *Action {
	ac := &Action{
		name:      name,
		arguments: make(ArgumentSet),
	}

	return ac
}

func (a *Action) Name() string {
	return a.name
}

func (a *Action) TypeID() string {
	return "Action"
}

func (a *Action) AddArgument(arg *Argument) {
	a.arguments.Insert(arg)
}

func (a *Action) NewInstance() *ActionInstance {
	ac := &ActionInstance{
		model:     a,
		arguments: maps.Clone(a.arguments),
	}
	return ac
}
