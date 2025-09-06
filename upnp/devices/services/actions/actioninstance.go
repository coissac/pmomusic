package actions

import "github.com/beevik/etree"

type ActionInstance struct {
	model *Action

	arguments ArgumentSet
}

func (a *ActionInstance) Name() string {
	return a.model.Name()
}

func (a *ActionInstance) TypeID() string {
	return "ActionInstance"
}

func (a *ActionInstance) ToXMLElement() *etree.Element {
	elem := etree.NewElement("action")

	name := elem.CreateElement("name")
	name.SetText(a.Name())

	elem.AddChild(a.arguments.ToXMLElement())
	return elem
}
