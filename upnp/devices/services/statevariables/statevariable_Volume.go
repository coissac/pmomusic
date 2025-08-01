package statevariables

var Volume = func() *StateVariable {

	vol := StateType_UI2.NewStateValue("Volume")

	vol.SetRange(0, 100)
	vol.SetStep(1)

	vol.SetSendingEvents()

	return vol
}()
