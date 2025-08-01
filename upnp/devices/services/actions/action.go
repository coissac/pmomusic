package actions

import "github.com/beevik/etree"

type Action struct {
	name string

	arguments ArgumentSet
}

func NewAction(name string) *Action {
	ac := &Action{
		name: name,
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

func (a *Action) ToXMLElement() *etree.Element {
	elem := etree.NewElement("action")

	name := elem.CreateElement("name")
	name.SetText(a.Name())

	elem.AddChild(a.arguments.ToXMLElement())
	return elem
}
