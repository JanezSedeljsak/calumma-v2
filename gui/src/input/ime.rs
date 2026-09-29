use std::cell::{Cell, RefCell};
use std::rc::Rc;
use winit::event::Ime;

pub enum ImeEvent {
    Preedit(String, Option<usize>),
    Commit(String),
}

#[derive(Clone, Default)]
pub struct ImeQueue {
    events: Rc<RefCell<Vec<ImeEvent>>>,
    board_owns: Rc<Cell<bool>>,
}

impl ImeQueue {
    pub fn set_board_owns(&self, owns: bool) {
        self.board_owns.set(owns);
    }

    pub fn board_owns(&self) -> bool {
        self.board_owns.get()
    }

    pub fn offer(&self, ime: &Ime) -> bool {
        if !self.board_owns.get() {
            return false;
        }
        let event = match ime {
            Ime::Preedit(text, cursor) => {
                ImeEvent::Preedit(text.clone(), cursor.map(|(_, end)| end))
            }
            Ime::Commit(text) => ImeEvent::Commit(text.clone()),
            Ime::Enabled | Ime::Disabled => return false,
        };
        self.events.borrow_mut().push(event);
        true
    }

    pub fn take(&self) -> Vec<ImeEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }
}
