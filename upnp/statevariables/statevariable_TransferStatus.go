package statevariables

var TransferStatus = func() *StateVariable {

	ts := StateType_String.NewStateValue("TransferStatus")

	ts.SetAllowedValues(
		"COMPLETED",
		"ERROR",
		"IN_PROGRESS",
		"NONE",
	)

	ts.SetSendingEvents()

	return ts
}()
