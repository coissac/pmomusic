use crate::variable_types::StateVarType;

pub trait UpnpVarType {
    fn as_state_var_type(&self) -> StateVarType;

    fn bit_size(&self) -> Option<usize> {
        self.as_state_var_type().bit_size()
    }

    fn is_numeric(&self) -> bool {
        self.as_state_var_type().is_numeric()
    }

    fn is_integer(&self) -> bool {
        self.as_state_var_type().is_integer()
    }

    fn is_signed_int(&self) -> bool {
        self.as_state_var_type().is_signed_int()
    }

    fn is_unsigned_int(&self) -> bool {
        self.as_state_var_type().is_unsigned_int()
    }

    fn is_float(&self) -> bool {
        self.as_state_var_type().is_float()
    }

    fn is_bool(&self) -> bool {
        self.as_state_var_type().is_bool()
    }

    fn is_string(&self) -> bool {
        self.as_state_var_type().is_string()
    }

    fn is_time(&self) -> bool {
        self.as_state_var_type().is_time()
    }

    fn is_uuid(&self) -> bool {
        self.as_state_var_type().is_uuid()
    }

    fn is_uri(&self) -> bool {
        self.as_state_var_type().is_uri()
    }

    fn is_binary(&self) -> bool {
        self.as_state_var_type().is_binary()
    }

    fn is_comparable(&self) -> bool {
        self.as_state_var_type().is_comparable()
    }
}
