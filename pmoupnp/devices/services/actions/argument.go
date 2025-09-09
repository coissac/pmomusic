package actions

import (
	"gargoton.petite-maison-orange.fr/eric/pmomusic/pmoupnp/devices/services/statevariables"
	"github.com/beevik/etree"
)

type Argument struct {
	name           string
	statevariables *statevariables.StateVariable
	in             bool
	out            bool
}

func newArgument(name string, statevariable *statevariables.StateVariable) *Argument {
	arg := &Argument{
		name:           name,
		statevariables: statevariable,
	}
	return arg
}

func NewInArgument(name string, statevariable *statevariables.StateVariable) *Argument {
	arg := newArgument(name, statevariable)
	arg.in = true
	return arg
}

func NewOutArgument(name string, statevariable *statevariables.StateVariable) *Argument {
	arg := newArgument(name, statevariable)
	arg.out = true
	return arg
}

func NewInOutArgument(name string, statevariable *statevariables.StateVariable) *Argument {
	arg := newArgument(name, statevariable)
	arg.in = true
	arg.out = true
	return arg
}

func (a *Argument) Name() string {
	return a.name
}

func (sv Argument) TypeID() string {
	return "Argument"
}

func (a *Argument) StateVariable() *statevariables.StateVariable {
	return a.statevariables
}

func (a *Argument) IsIn() bool {
	return a.in
}

func (a *Argument) IsOut() bool {
	return a.out
}

func (a *Argument) ToXMLElement() *etree.Element {
	var elem, arg *etree.Element

	if a.IsIn() && a.IsOut() {
		elem = etree.NewElement("")
		arg = elem.CreateElement("argument")
	} else {
		elem = etree.NewElement("argument")
		arg = elem
	}

	if a.IsIn() {
		name := arg.CreateElement("name")
		name.SetText(a.Name())

		direction := arg.CreateElement("direction")
		direction.SetText("in")

		relatedStateVariable := arg.CreateElement("relatedStateVariable")
		relatedStateVariable.SetText(a.StateVariable().Name())
	}

	if a.IsIn() && a.IsOut() {
		arg = elem.CreateElement("argument")
	}

	if a.IsOut() {
		name := arg.CreateElement("name")
		name.SetText(a.Name())

		direction := arg.CreateElement("direction")
		direction.SetText("out")

		relatedStateVariable := arg.CreateElement("relatedStateVariable")
		relatedStateVariable.SetText(a.StateVariable().Name())
	}

	return elem
}
