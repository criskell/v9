use crate::common::globals::StoreOrigin;
use crate::handles::handles::Handle;
use crate::execution::isolate::Isolate;
use crate::objects::hash_table::K_NOT_FOUND;
use crate::objects::instance_type;
use crate::objects::js_objects::JSObject;
use crate::objects::map::Map;
use crate::objects::property_details::{PropertyDetails, Representation};
use crate::objects::tagged::Tagged;


#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum State {
    NotFound,
    Accessor,
    Data,
    Transition,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Configuration {
    OwnProperty,
    PrototypeChain,
}

pub const K_INVALID_INDEX: i32 = -1;

pub struct LookupIterator {
    state: State,
    has_property: bool,
    property_details: PropertyDetails,
    receiver: Handle,
    name: Handle,
    holder: Handle,
    lookup_start_object: Handle,
    number: i32,
    configuration: Configuration,
    transition: Handle,
}

impl LookupIterator {
    pub fn new(isolate: &mut Isolate, receiver: Tagged, name: Tagged) -> LookupIterator {
        LookupIterator::with_configuration(isolate, receiver, name, Configuration::PrototypeChain)
    }

    pub fn own_property(isolate: &mut Isolate, receiver: Tagged, name: Tagged) -> LookupIterator {
        LookupIterator::with_configuration(isolate, receiver, name, Configuration::OwnProperty)
    }

    pub fn with_configuration(
        isolate: &mut Isolate,
        receiver: Tagged,
        name: Tagged,
        configuration: Configuration,
    ) -> LookupIterator {
        LookupIterator::with_lookup_start_object(isolate, receiver, name, receiver, configuration)
    }

    pub fn with_lookup_start_object(
        isolate: &mut Isolate,
        receiver: Tagged,
        name: Tagged,
        lookup_start_object: Tagged,
        configuration: Configuration,
    ) -> LookupIterator {
        let receiver = isolate.create_handle(receiver);
        let name = isolate.create_handle(name);
        let lookup_start_object = isolate.create_handle(lookup_start_object);
        let mut iterator = LookupIterator {
            state: State::NotFound,
            has_property: false,
            property_details: PropertyDetails::empty(),
            receiver,
            name,
            holder: lookup_start_object,
            lookup_start_object,
            number: K_INVALID_INDEX,
            configuration,
            transition: name,
        };
        iterator.start(isolate);
        iterator
    }

    fn start(&mut self, isolate: &mut Isolate) {
        self.state = State::NotFound;
        self.has_property = false;
        let Some(holder) = self.lookup_start_object.get(isolate).to_strong_heap_object() else {
            return;
        };
        if !instance_type::is_js_receiver(isolate.heap.instance_type(holder)) {
            return;
        }
        self.holder = self.lookup_start_object;
        let map = Map::unchecked_cast(isolate.heap.map(holder));
        self.state = self.lookup_in_regular_holder(isolate, map, JSObject::unchecked_cast(holder));
        if !self.is_found() {
            self.next(isolate);
        }
    }

    pub fn next(&mut self, isolate: &mut Isolate) {
        debug_assert_ne!(self.state, State::Transition);
        self.has_property = false;
        let mut holder = JSObject::unchecked_cast(self.holder.object(isolate));
        let mut map = holder.map(&isolate.heap);
        loop {
            let Some(next) = self.next_holder(isolate, map) else {
                self.state = State::NotFound;
                return;
            };
            holder = next;
            map = holder.map(&isolate.heap);
            self.holder = isolate.create_object_handle(holder.object());
            self.state = self.lookup_in_regular_holder(isolate, map, holder);
            if self.is_found() {
                return;
            }
        }
    }

    fn next_holder(&self, isolate: &Isolate, map: Map) -> Option<JSObject> {
        let prototype = map.prototype(&isolate.heap);
        if prototype == isolate.roots.null_value {
            return None;
        }
        if self.configuration == Configuration::OwnProperty {
            return None;
        }
        let object = prototype.to_strong_heap_object()?;
        if !instance_type::is_js_receiver(isolate.heap.instance_type(object)) {
            return None;
        }
        Some(JSObject::unchecked_cast(object))
    }

    fn lookup_in_regular_holder(
        &mut self,
        isolate: &Isolate,
        map: Map,
        holder: JSObject,
    ) -> State {
        let name = self.name.get(isolate);
        if !map.is_dictionary_map(&isolate.heap) {
            let descriptors = map.instance_descriptors(&isolate.heap);
            self.number = descriptors.search_within(
                &isolate.heap,
                name,
                map.number_of_own_descriptors(&isolate.heap),
            );
            if self.number == crate::objects::descriptor_array::K_NOT_FOUND {
                return State::NotFound;
            }
            self.property_details = descriptors.get_details(&isolate.heap, self.number);
        } else {
            let dictionary = holder.property_dictionary(&isolate.heap);
            self.number = dictionary.find_entry(&isolate.heap, name);
            if self.number == K_NOT_FOUND {
                return State::NotFound;
            }
            self.property_details = dictionary.details_at(&isolate.heap, self.number);
        }
        self.has_property = true;
        match self.property_details.kind() {
            crate::objects::property_details::PropertyKind::Data => State::Data,
            crate::objects::property_details::PropertyKind::Accessor => State::Accessor,
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn is_found(&self) -> bool {
        self.state != State::NotFound
    }

    pub fn not_found(&mut self) {
        self.has_property = false;
        self.state = State::NotFound;
    }

    pub fn name(&self, isolate: &Isolate) -> Tagged {
        self.name.get(isolate)
    }

    pub fn get_receiver(&self, isolate: &Isolate) -> Tagged {
        self.receiver.get(isolate)
    }

    pub fn get_holder(&self, isolate: &Isolate) -> JSObject {
        JSObject::unchecked_cast(self.holder.object(isolate))
    }

    pub fn holder_is_receiver(&self, isolate: &Isolate) -> bool {
        self.holder.get(isolate) == self.receiver.get(isolate)
    }

    pub fn is_dictionary_holder(&self, isolate: &Isolate) -> bool {
        !self.get_holder(isolate).has_fast_properties(&isolate.heap)
    }

    pub fn property_details(&self) -> PropertyDetails {
        debug_assert!(self.has_property);
        self.property_details
    }

    pub fn is_read_only(&self) -> bool {
        self.property_details().is_read_only()
    }

    pub fn is_configurable(&self) -> bool {
        self.property_details().is_configurable()
    }

    pub fn number(&self) -> i32 {
        self.number
    }

    fn fetch_value(&self, isolate: &Isolate) -> Tagged {
        let holder = self.get_holder(isolate);
        let value = if self.is_dictionary_holder(isolate) {
            holder
                .property_dictionary(&isolate.heap)
                .value_at(&isolate.heap, self.number)
        } else if self.property_details.location()
            == crate::objects::property_details::PropertyLocation::Descriptor
        {
            holder
                .map(&isolate.heap)
                .instance_descriptors(&isolate.heap)
                .get_value(&isolate.heap, self.number)
        } else {
            holder.fast_property_at(&isolate.heap, self.property_details.field_index() as i32)
        };
        isolate.unwrap_cell(holder, value)
    }

    pub fn get_data_value(&self, isolate: &Isolate) -> Tagged {
        debug_assert_eq!(self.state, State::Data);
        self.fetch_value(isolate)
    }

    pub fn get_accessors(&self, isolate: &Isolate) -> Tagged {
        debug_assert_eq!(self.state, State::Accessor);
        self.fetch_value(isolate)
    }

    pub fn transition_map(&self, isolate: &Isolate) -> Map {
        debug_assert_eq!(self.state, State::Transition);
        Map::unchecked_cast(self.transition.object(isolate))
    }

    pub fn prepare_for_data_property(&mut self, isolate: &mut Isolate, value: Tagged) {
        debug_assert!(self.state == State::Data || self.state == State::Accessor);
        debug_assert!(self.holder_is_receiver(isolate));
        let holder = self.get_holder(isolate);
        if !holder.has_fast_properties(&isolate.heap) {
            return;
        }
        let map = holder.map(&isolate.heap);
        let name = self.name.get(isolate);
        isolate.widen_field_representation(map, name, value);
    }

    pub fn prepare_transition_to_data_property(
        &mut self,
        isolate: &mut Isolate,
        receiver: JSObject,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
        store_origin: StoreOrigin,
    ) {
        use crate::objects::property_details::{PropertyConstness, PropertyKind};
        if self.state == State::Transition {
            return;
        }
        debug_assert!(self.state == State::NotFound || !self.holder_is_receiver(isolate));

        let map = receiver.map(&isolate.heap);
        if map.is_dictionary_map(&isolate.heap) {
            self.state = State::Transition;
            self.property_details = PropertyDetails::new_dictionary(
                PropertyKind::Data,
                attributes,
                PropertyConstness::Mutable,
                0,
            );
            self.transition = isolate.create_object_handle(map.object());
            return;
        }

        let name = self.name.get(isolate);
        let representation = Representation::for_value(value);
        let transition =
            isolate.transition_to_data_property(map, name, representation, attributes, store_origin);
        self.state = State::Transition;
        self.transition = isolate.create_object_handle(transition.object());
        if !transition.is_dictionary_map(&isolate.heap) {
            let name = self.name.get(isolate);
            self.property_details = transition
                .find_property_details(&isolate.heap, name)
                .expect("the transition target owns the property");
            self.has_property = true;
        }
    }

    pub fn apply_transition_to_data_property(
        &mut self,
        isolate: &mut Isolate,
        receiver: JSObject,
    ) {
        debug_assert_eq!(self.state, State::Transition);
        self.holder = isolate.create_object_handle(receiver.object());
        let transition = self.transition_map(isolate);
        isolate.migrate_to_map(receiver, transition);
        let receiver = JSObject::unchecked_cast(self.holder.object(isolate));
        let transition = self.transition_map(isolate);
        if transition.is_dictionary_map(&isolate.heap) {
            let name = self.name.get(isolate);
            let details = self.property_details;
            let undefined = isolate.undefined_value();
            isolate.set_normalized_property(receiver, name, undefined, details);
            let receiver = JSObject::unchecked_cast(self.holder.object(isolate));
            let name = self.name.get(isolate);
            self.number = receiver
                .property_dictionary(&isolate.heap)
                .find_entry(&isolate.heap, name);
            self.property_details = receiver
                .property_dictionary(&isolate.heap)
                .details_at(&isolate.heap, self.number);
        } else {
            self.number = transition.last_added(&isolate.heap);
        }
        self.has_property = true;
        self.state = State::Data;
    }

    pub fn delete(&mut self, isolate: &mut Isolate) {
        let holder = self.get_holder(isolate);
        let mode = holder.map(&isolate.heap).is_prototype_map(&isolate.heap);
        let _ = mode;
        if holder.has_fast_properties(&isolate.heap) {
            isolate.normalize_properties(holder);
        }
        let holder = JSObject::unchecked_cast(self.holder.object(isolate));
        let name = self.name.get(isolate);
        let entry = holder
            .property_dictionary(&isolate.heap)
            .find_entry(&isolate.heap, name);
        if entry != K_NOT_FOUND {
            isolate.delete_normalized_property(holder, entry);
        }
        self.state = State::NotFound;
    }

    pub fn write_data_value(&self, isolate: &mut Isolate, value: Tagged) {
        debug_assert_eq!(self.state, State::Data);
        let holder = self.get_holder(isolate);
        if self.is_dictionary_holder(isolate) {
            holder
                .property_dictionary(&isolate.heap)
                .value_at_put(&mut isolate.heap, self.number, value);
            return;
        }
        holder.set_fast_property_at(
            &mut isolate.heap,
            self.property_details.field_index() as i32,
            value,
        );
    }
}

impl Isolate {




    pub fn get_data_property(&mut self, object: JSObject, name: Tagged) -> Tagged {
        if let Some(length) = self.array_length_if_requested(object, name) {
            return length;
        }
        let it = LookupIterator::new(self, object.tagged(), name);
        match it.state() {
            State::Data => it.get_data_value(self),
            State::Accessor => it.get_accessors(self),
            _ => self.undefined_value(),
        }
    }

    fn private_name_description(&self, name: Tagged) -> Option<(std::string::String, bool)> {
        use crate::objects::name::Symbol;
        let object = name.to_strong_heap_object()?;
        if self.heap.instance_type(object) != crate::objects::instance_type::SYMBOL_TYPE {
            return None;
        }
        let symbol = Symbol::unchecked_cast(object);
        if !symbol.is_private(&self.heap) {
            return None;
        }
        let description = symbol.description(&self.heap);
        let text = match crate::objects::string::String::try_cast(&self.heap, description) {
            Some(string) => string.to_rust_string(&self.heap),
            None => std::string::String::new(),
        };
        Some((text, symbol.is_private_brand(&self.heap)))
    }

    pub(crate) fn get_property(
        &mut self,
        it: &mut LookupIterator,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::interpreter::interpreter::Thrown;
        while it.is_found() {
            match it.state() {
                State::Transition => unreachable!("a transition is not a lookup state"),
                State::Accessor => {
                    let pair = it.get_accessors(self);
                    let receiver = it.get_receiver(self);
                    let getter = self.accessor_component(pair, true);
                    return self.call_accessor(getter, receiver, &[]);
                }
                State::Data => {
                    return Ok(it.get_data_value(self));
                }
                State::NotFound => {}
            }
            it.next(self);
        }
        let name = it.name(self);
        if let Some((description, is_private_brand)) = self.private_name_description(name) {
            let message = if is_private_brand {
                let class_name = if description.is_empty() {
                    "anonymous".to_string()
                } else {
                    description
                };
                self.new_string(&format!("Receiver must be an instance of class {class_name}"))
            } else {
                self.new_string(&format!(
                    "Cannot read private member {description} from an object whose class did \
                     not declare it"
                ))
            };
            return Err(Thrown(message.tagged()));
        }
        Ok(self.undefined_value())
    }

    pub(crate) fn get_property_with_accessors(
        &mut self,
        object: JSObject,
        name: Tagged,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        let mut it = LookupIterator::new(self, object.tagged(), name);
        self.get_property(&mut it)
    }

    pub(crate) fn set_property(
        &mut self,
        it: &mut LookupIterator,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        if it.is_found() {
            let mut found = true;
            let result = self.set_property_internal(it, value, store_origin, &mut found)?;
            if found {
                return Ok(result);
            }
        }
        self.add_data_property(
            it,
            value,
            crate::objects::property_details::NONE,
            store_origin,
        )
    }

    fn set_property_internal(
        &mut self,
        it: &mut LookupIterator,
        value: Tagged,
        _store_origin: StoreOrigin,
        found: &mut bool,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        loop {
            match it.state() {
                State::Accessor => {
                    if it.is_read_only() {
                        return self.write_to_read_only_property(it);
                    }
                    let pair = it.get_accessors(self);
                    let receiver = it.get_receiver(self);
                    let setter = self.accessor_component(pair, false);
                    self.call_accessor(setter, receiver, &[value])?;
                    return Ok(true);
                }
                State::Data => {
                    if it.is_read_only() {
                        return self.write_to_read_only_property(it);
                    }
                    if it.holder_is_receiver(self) {
                        return self.set_data_property(it, value);
                    }
                    *found = false;
                    return Ok(false);
                }
                State::NotFound | State::Transition => {
                    *found = false;
                    return Ok(false);
                }
            }
        }
    }

    fn set_data_property(
        &mut self,
        it: &mut LookupIterator,
        value: Tagged,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        it.prepare_for_data_property(self, value);
        it.write_data_value(self, value);
        Ok(true)
    }

    fn add_data_property(
        &mut self,
        it: &mut LookupIterator,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
        store_origin: StoreOrigin,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        let receiver = it.get_receiver(self);
        let Some(receiver) = receiver.to_strong_heap_object() else {
            return self.cannot_create_property(it);
        };
        if !instance_type::is_js_receiver(self.heap.instance_type(receiver)) {
            return self.cannot_create_property(it);
        }
        let receiver = JSObject::unchecked_cast(receiver);

        let name = it.name(self);
        if let Some((description, _)) = self.private_name_description(name) {
            if !it.is_found() {
                let message = self
                    .new_string(&format!(
                        "Cannot write private member {description} to an object whose class did \
                         not declare it"
                    ))
                    .tagged();
                return Err(crate::interpreter::interpreter::Thrown(message));
            }
        }

        if !receiver.map(&self.heap).is_extensible(&self.heap) {
            return Ok(false);
        }

        self.transition_and_write_data_property(it, receiver, value, attributes, store_origin)
    }

    fn transition_and_write_data_property(
        &mut self,
        it: &mut LookupIterator,
        receiver: JSObject,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
        store_origin: StoreOrigin,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        let receiver_handle = self.create_object_handle(receiver.object());
        let value_handle = self.create_handle(value);
        it.prepare_transition_to_data_property(self, receiver, value, attributes, store_origin);
        debug_assert_eq!(it.state(), State::Transition);
        let receiver = JSObject::unchecked_cast(receiver_handle.object(self));
        it.apply_transition_to_data_property(self, receiver);
        let value = value_handle.get(self);
        it.write_data_value(self, value);
        Ok(true)
    }

    fn write_to_read_only_property(
        &mut self,
        _it: &mut LookupIterator,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        Ok(false)
    }

    fn cannot_create_property(
        &mut self,
        _it: &mut LookupIterator,
    ) -> Result<bool, crate::interpreter::interpreter::Thrown> {
        Ok(false)
    }

    pub(crate) fn set_property_with_accessors(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        if self.array_length_if_requested(object, name).is_some() {
            self.array_length_setter(object, value);
            return Ok(value);
        }
        if self.is_global_object(object) {
            self.store_global(name, value);
            return Ok(value);
        }
        let scope = self.handle_scope();
        let value_handle = self.create_handle(value);
        let mut it = LookupIterator::new(self, object.tagged(), name);
        let value = value_handle.get(self);
        let result = self.set_property(&mut it, value, store_origin);
        let value = value_handle.get(self);
        self.close_handle_scope(scope);
        result?;
        Ok(value)
    }


    fn accessor_component(&self, pair: Tagged, getter: bool) -> Tagged {
        use crate::objects::r#struct::AccessorPair;
        match pair.to_strong_heap_object() {
            Some(object)
                if self.heap.instance_type(object)
                    == crate::objects::instance_type::ACCESSOR_PAIR_TYPE =>
            {
                let pair = AccessorPair::unchecked_cast(object);
                let accessor = if getter {
                    pair.getter(&self.heap)
                } else {
                    pair.setter(&self.heap)
                };
                if self.heap.is_null(accessor) {
                    return self.undefined_value();
                }
                accessor
            }
            _ => self.undefined_value(),
        }
    }

    fn call_accessor(
        &mut self,
        accessor: Tagged,
        receiver: Tagged,
        args: &[Tagged],
    ) -> crate::interpreter::interpreter::ExecutionResult {
        let Some(callable) = accessor.to_strong_heap_object() else {
            return Ok(self.undefined_value());
        };
        if !crate::objects::instance_type::is_callable(self.heap.instance_type(callable)) {
            return Ok(self.undefined_value());
        }
        let function = crate::objects::js_function::JSFunction::unchecked_cast(callable);
        self.execute(function, receiver, args)
    }

    pub fn get_own_property(&mut self, object: JSObject, name: Tagged) -> Option<Tagged> {
        let it = LookupIterator::own_property(self, object.tagged(), name);
        match it.state() {
            State::Data => Some(it.get_data_value(self)),
            State::Accessor => Some(it.get_accessors(self)),
            _ => None,
        }
    }


    pub(crate) fn define_accessor(
        &mut self,
        object: JSObject,
        name: Tagged,
        getter: Tagged,
        setter: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let name_handle = self.create_handle(name);
        let getter_handle = self.create_handle(getter);
        let setter_handle = self.create_handle(setter);

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let name = name_handle.get(self);
        let existing = self.existing_accessor_pair(object, name);
        let pair = match existing {
            Some(pair) => {
                let getter = getter_handle.get(self);
                let setter = setter_handle.get(self);
                if pair.equals(&self.heap, getter, setter) {
                    let it = LookupIterator::own_property(self, object.tagged(), name);
                    if it.property_details().attributes() == attributes {
                        self.close_handle_scope(scope);
                        return;
                    }
                    pair
                } else {
                    let copy = self.accessor_pair_copy(pair);
                    let getter = getter_handle.get(self);
                    let setter = setter_handle.get(self);
                    copy.set_components(&mut self.heap, getter, setter);
                    copy
                }
            }
            None => {
                let fresh = self.new_accessor_pair();
                let getter = getter_handle.get(self);
                let setter = setter_handle.get(self);
                fresh.set_components(&mut self.heap, getter, setter);
                fresh
            }
        };
        let pair_handle = self.create_object_handle(pair.object());

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let map = object.map(&self.heap);
        let name = name_handle.get(self);
        let pair_value = pair_handle.get(self);
        if map.is_dictionary_map(&self.heap) {
            self.transition_to_accessor_pair(object, name, pair_value, attributes);
        } else {
            let new_map = self.transition_to_accessor_property(map, name, pair_value, attributes);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.heap.set_map(object.object(), new_map.object());
        }
        self.close_handle_scope(scope);
    }

    fn transition_to_accessor_pair(
        &mut self,
        object: JSObject,
        name: Tagged,
        pair: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) {
        use crate::objects::property_details::{PropertyConstness, PropertyKind};

        let details = PropertyDetails::new_dictionary(
            PropertyKind::Accessor,
            attributes,
            PropertyConstness::Mutable,
            0,
        );

        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let name_handle = self.create_handle(name);
        let pair_handle = self.create_handle(pair);
        self.normalize_properties(object);
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let name = name_handle.get(self);
        let pair = pair_handle.get(self);
        self.set_normalized_property(object, name, pair, details);
        self.close_handle_scope(scope);
    }

    fn existing_accessor_pair(
        &mut self,
        object: JSObject,
        name: Tagged,
    ) -> Option<crate::objects::r#struct::AccessorPair> {
        let it = LookupIterator::own_property(self, object.tagged(), name);
        if it.state() != State::Accessor {
            return None;
        }
        let value = it.get_accessors(self);
        Some(crate::objects::r#struct::AccessorPair::unchecked_cast(
            value.to_strong_heap_object()?,
        ))
    }

    pub(crate) fn new_accessor_pair(&mut self) -> crate::objects::r#struct::AccessorPair {
        use crate::objects::r#struct::AccessorPair;
        let object = self.allocate_raw_with_retry_or_fail(
            AccessorPair::K_SIZE as usize,
            crate::heap::heap::AllocationType::Old,
        );
        let map = self
            .roots
            .accessor_pair_map
            .to_strong_heap_object()
            .unwrap();
        self.heap.set_map(object, map);
        let pair = AccessorPair::unchecked_cast(object);
        let null = self.null_value();
        pair.set_getter(&mut self.heap, null);
        pair.set_setter(&mut self.heap, null);
        pair
    }

    fn accessor_pair_copy(
        &mut self,
        pair: crate::objects::r#struct::AccessorPair,
    ) -> crate::objects::r#struct::AccessorPair {
        let scope = self.handle_scope();
        let pair_handle = self.create_object_handle(pair.object());
        let copy = self.new_accessor_pair();
        let pair = crate::objects::r#struct::AccessorPair::unchecked_cast(pair_handle.object(self));
        let getter = pair.getter(&self.heap);
        let setter = pair.setter(&self.heap);
        copy.set_getter(&mut self.heap, getter);
        copy.set_setter(&mut self.heap, setter);
        self.close_handle_scope(scope);
        copy
    }

    #[inline]

    pub(crate) fn array_length_if_requested(
        &self,
        object: JSObject,
        name: Tagged,
    ) -> Option<Tagged> {
        use crate::objects::instance_type::JS_ARRAY_TYPE;
        if self.heap.instance_type(object.object()) != JS_ARRAY_TYPE {
            return None;
        }
        let string = crate::objects::string::String::try_cast(&self.heap, name)?;
        if string.to_rust_string(&self.heap) != "length" {
            return None;
        }
        let array = crate::objects::js_array::JSArray::unchecked_cast(object.object());
        Some(self.array_length_getter(array))
    }

    #[inline]
    fn unwrap_cell(&self, holder: JSObject, value: Tagged) -> Tagged {
        if !self.is_global_object(holder) {
            return value;
        }
        match value.to_strong_heap_object() {
            Some(object)
                if self.heap.instance_type(object) == instance_type::PROPERTY_CELL_TYPE =>
            {
                crate::objects::property_cell::PropertyCell::unchecked_cast(object)
                    .value(&self.heap)
            }
            _ => value,
        }
    }

    pub fn set_property_by_name(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
    ) -> bool {
        self.set_property_with_origin(object, name, value, StoreOrigin::Named)
    }

    pub fn set_property_with_origin(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> bool {
        if self.array_length_if_requested(object, name).is_some() {
            self.array_length_setter(object, value);
            return true;
        }
        if self.is_global_object(object) {
            self.store_global(name, value);
            return true;
        }
        self.set_property_ignoring_cells(object, name, value, store_origin)
    }

    pub(crate) fn set_property_ignoring_cells(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        store_origin: StoreOrigin,
    ) -> bool {
        let map = object.map(&self.heap);

        if map.is_dictionary_map(&self.heap) {
            return self.set_dictionary_property(object, name, value);
        }

        if let Some(details) = map.find_property_details(&self.heap, name) {
            if details.is_read_only() {
                return false;
            }
            let field_index = details.field_index() as i32;
            object.set_fast_property_at(&mut self.heap, field_index, value);
            self.widen_field_representation(map, name, value);
            return true;
        }

        if !map.is_extensible(&self.heap) {
            return false;
        }

        if self.too_many_fast_properties(map, store_origin) {
            let scope = self.handle_scope();
            let object_handle = self.create_object_handle(object.object());
            let name_handle = self.create_handle(name);
            let value_handle = self.create_handle(value);
            self.normalize_properties(object);
            let object = JSObject::unchecked_cast(object_handle.object(self));
            let name = name_handle.get(self);
            let value = value_handle.get(self);
            let result = self.set_dictionary_property(object, name, value);
            self.close_handle_scope(scope);
            return result;
        }

        let representation = Representation::for_value(value);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let name_handle = self.create_handle(name);
        let value_handle = self.create_handle(value);

        let new_map = self.transition_to_data_property(
            map,
            name,
            representation,
            crate::objects::property_details::NONE,
            store_origin,
        );
        let new_map_handle = self.create_object_handle(new_map.object());
        let name = name_handle.get(self);
        let field_index = new_map
            .find_property_details(&self.heap, name)
            .expect("the transition target must own the property")
            .field_index() as i32;

        let inobject = new_map.inobject_properties(&self.heap);
        if field_index >= inobject {
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.grow_property_array(object, field_index - inobject + 1);
        }

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        let value = value_handle.get(self);
        self.heap.set_map(object.object(), new_map.object());
        object.set_fast_property_at(&mut self.heap, field_index, value);
        self.close_handle_scope(scope);
        true
    }

    pub(crate) fn normalize_properties(&mut self, object: JSObject) {
        let map = object.map(&self.heap);
        if map.is_dictionary_map(&self.heap) {
            return;
        }
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let new_map = self.normalize(map);
        let new_map_handle = self.create_object_handle(new_map.object());
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        self.migrate_fast_to_slow(object, new_map);
        self.close_handle_scope(scope);
    }

    pub(crate) fn migrate_fast_to_slow(&mut self, object: JSObject, new_map: Map) {
        let map = object.map(&self.heap);
        let number_of_own = map.number_of_own_descriptors(&self.heap);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let map_handle = self.create_object_handle(map.object());
        let mut dictionary = self.new_name_dictionary(number_of_own as u32);
        for index in 0..number_of_own {
            let map = Map::unchecked_cast(map_handle.object(self));
            let descriptors = map.instance_descriptors(&self.heap);
            let key = descriptors.get_key(&self.heap, index);
            let details = descriptors.get_details(&self.heap, index);
            let value = if details.location()
                == crate::objects::property_details::PropertyLocation::Field
            {
                let object = JSObject::unchecked_cast(object_handle.object(self));
                object.fast_property_at(&self.heap, details.field_index() as i32)
            } else {
                descriptors.get_value(&self.heap, index)
            };
            let dictionary_handle = self.create_object_handle(dictionary.object());
            let slow_details = PropertyDetails::new_dictionary(
                details.kind(),
                details.attributes(),
                crate::objects::property_details::PropertyConstness::Mutable,
                0,
            );
            let dictionary_now = crate::objects::dictionary::NameDictionary::unchecked_cast(
                dictionary_handle.object(self),
            );
            dictionary = self.name_dictionary_add(dictionary_now, key, value, slow_details);
        }
        let dictionary_handle = self.create_object_handle(dictionary.object());
        let object = JSObject::unchecked_cast(object_handle.object(self));
        self.heap.set_map(object.object(), new_map.object());
        let dictionary = dictionary_handle.get(self);
        object.set_raw_properties_or_hash(&mut self.heap, dictionary);
        self.close_handle_scope(scope);
    }

    pub(crate) fn set_dictionary_property(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
    ) -> bool {
        let dictionary = object.property_dictionary(&self.heap);
        let entry = dictionary.find_entry(&self.heap, name);
        if entry != K_NOT_FOUND {
            if dictionary.details_at(&self.heap, entry).is_read_only() {
                return false;
            }
            dictionary.value_at_put(&mut self.heap, entry, value);
            return true;
        }
        if !object.map(&self.heap).is_extensible(&self.heap) {
            return false;
        }
        let details = PropertyDetails::new_dictionary(
            crate::objects::property_details::PropertyKind::Data,
            crate::objects::property_details::NONE,
            crate::objects::property_details::PropertyConstness::Mutable,
            0,
        );
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let updated = self
            .name_dictionary_add(dictionary, name, value, details)
            .tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        object.set_raw_properties_or_hash(&mut self.heap, updated);
        self.close_handle_scope(scope);
        true
    }

    pub(crate) fn delete_property(&mut self, object: JSObject, name: Tagged) -> bool {
        let mut it = LookupIterator::own_property(self, object.tagged(), name);
        if !it.is_found() {
            return true;
        }
        if !it.is_configurable() {
            return false;
        }
        it.delete(self);
        true
    }

    pub(crate) fn define_own_property(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
    ) -> bool {
        let it = LookupIterator::own_property(self, object.tagged(), name);
        {
            if it.state() == State::Accessor {
                let scope = self.handle_scope();
                let object_handle = self.create_object_handle(object.object());
                let name_handle = self.create_handle(name);
                let value_handle = self.create_handle(value);
                self.delete_property(object, name);
                let object = JSObject::unchecked_cast(object_handle.object(self));
                let name = name_handle.get(self);
                let value = value_handle.get(self);
                let result = self.add_property_with_attributes(
                    object,
                    name,
                    value,
                    crate::objects::property_details::NONE,
                );
                self.close_handle_scope(scope);
                return result;
            }
        }
        self.set_property_with_origin(object, name, value, StoreOrigin::Named)
    }

    pub(crate) fn add_property_with_attributes(
        &mut self,
        object: JSObject,
        name: Tagged,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) -> bool {
        let map = object.map(&self.heap);
        if map.is_dictionary_map(&self.heap) {
            return self.set_dictionary_property(object, name, value);
        }
        if map.find_property_details(&self.heap, name).is_some() {
            return self.set_property_ignoring_cells(object, name, value, StoreOrigin::Named);
        }

        let representation = Representation::for_value(value);
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let name_handle = self.create_handle(name);

        let new_map = self.transition_to_data_property(
            map,
            name,
            representation,
            attributes,
            StoreOrigin::Named,
        );
        let new_map_handle = self.create_object_handle(new_map.object());
        let name = name_handle.get(self);
        let field_index = new_map
            .find_property_details(&self.heap, name)
            .expect("the transition target must own the property")
            .field_index() as i32;

        let inobject = new_map.inobject_properties(&self.heap);
        if field_index >= inobject {
            let object = JSObject::unchecked_cast(object_handle.object(self));
            self.grow_property_array(object, field_index - inobject + 1);
        }

        let object = JSObject::unchecked_cast(object_handle.object(self));
        let new_map = Map::unchecked_cast(new_map_handle.object(self));
        let value = value_handle.get(self);
        self.heap.set_map(object.object(), new_map.object());
        object.set_fast_property_at(&mut self.heap, field_index, value);
        self.close_handle_scope(scope);
        true
    }

    pub(crate) fn add_property_by_str_with_attributes(
        &mut self,
        object: JSObject,
        name: &str,
        value: Tagged,
        attributes: crate::objects::property_details::PropertyAttributes,
    ) -> bool {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let value = value_handle.get(self);
        let result = self.add_property_with_attributes(object, key, value, attributes);
        self.close_handle_scope(scope);
        result
    }

    pub fn set_property_by_str(&mut self, object: JSObject, name: &str, value: Tagged) -> bool {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let value_handle = self.create_handle(value);
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let value = value_handle.get(self);
        let result = self.set_property_by_name(object, key, value);
        self.close_handle_scope(scope);
        result
    }

    pub fn get_property_by_str(&mut self, object: JSObject, name: &str) -> Tagged {
        let scope = self.handle_scope();
        let object_handle = self.create_object_handle(object.object());
        let key = self.internalize_string(name).tagged();
        let object = JSObject::unchecked_cast(object_handle.object(self));
        let result = self.get_data_property(object, key);
        self.close_handle_scope(scope);
        result
    }
}

#[cfg(test)]
mod tests {
    #[derive(Copy, Clone)]
    struct Rooted(usize);

    impl Rooted {
        fn new(isolate: &mut Isolate, object: JSObject) -> Rooted {
            isolate.handles.push(object.tagged());
            Rooted(isolate.handles.len() - 1)
        }

        fn get(self, isolate: &Isolate) -> JSObject {
            JSObject::unchecked_cast(isolate.handles[self.0].to_strong_heap_object().unwrap())
        }
    }

    use super::*;
    use crate::objects::descriptor_array::K_NOT_FOUND;
    use crate::objects::heap_number::number_value;
    use crate::objects::smi::Smi;
    use crate::objects::transitions::TransitionsAccessor;

    #[test]
    fn adding_a_property_moves_the_object_to_a_new_map() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let map_before = object.get(&isolate).map(&isolate.heap);

        assert!(isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1)));

        let map_after = object.get(&isolate).map(&isolate.heap);
        assert_ne!(map_before, map_after, "the shape changed");
        assert_eq!(map_before.number_of_own_descriptors(&isolate.heap), 0);
        assert_eq!(map_after.number_of_own_descriptors(&isolate.heap), 1);
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            1
        );
    }

    #[test]
    fn the_new_map_points_back_at_the_old_one() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let map0 = object.get(&isolate).map(&isolate.heap);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        let map1 = object.get(&isolate).map(&isolate.heap);
        isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(2));
        let map2 = object.get(&isolate).map(&isolate.heap);

        assert_eq!(map2.back_pointer(&isolate.heap), Some(map1));
        assert_eq!(map1.back_pointer(&isolate.heap), Some(map0));
        assert_eq!(map0.back_pointer(&isolate.heap), None, "the root map");
    }

    #[test]
    fn objects_built_the_same_way_share_a_map() {
        let mut isolate = Isolate::new();
        let a = isolate.new_js_object();
        let a = Rooted::new(&mut isolate, a);
        let b = isolate.new_js_object();
        let b = Rooted::new(&mut isolate, b);

        for rooted in [a, b] {
            let object = rooted.get(&isolate);
            isolate.set_property_by_str(object, "x", Smi::from_int(1));
            let object = rooted.get(&isolate);
            isolate.set_property_by_str(object, "y", Smi::from_int(2));
        }

        assert_eq!(
            a.get(&isolate).map(&isolate.heap),
            b.get(&isolate).map(&isolate.heap),
            "this is the entire point of hidden classes"
        );
    }

    #[test]
    fn property_order_produces_different_shapes() {
        let mut isolate = Isolate::new();
        let xy = isolate.new_js_object();
        let xy = Rooted::new(&mut isolate, xy);
        isolate.set_property_by_str(xy.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(xy.get(&isolate), "y", Smi::from_int(2));

        let yx = isolate.new_js_object();
        let yx = Rooted::new(&mut isolate, yx);
        isolate.set_property_by_str(yx.get(&isolate), "y", Smi::from_int(2));
        isolate.set_property_by_str(yx.get(&isolate), "x", Smi::from_int(1));

        assert_ne!(
            xy.get(&isolate).map(&isolate.heap),
            yx.get(&isolate).map(&isolate.heap),
            "{{x, y}} and {{y, x}} are different shapes -- the classic \
             performance footgun"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(xy.get(&isolate), "x")),
            1
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(yx.get(&isolate), "x")),
            1
        );
        let x_in_xy = xy.get(&isolate).map(&isolate.heap);
        let x_key = isolate.internalize_string("x").tagged();
        assert_eq!(
            x_in_xy
                .find_property_details(&isolate.heap, x_key)
                .unwrap()
                .field_index(),
            0
        );
        assert_eq!(
            yx.get(&isolate)
                .map(&isolate.heap)
                .find_property_details(&isolate.heap, x_key)
                .unwrap()
                .field_index(),
            1
        );
    }

    #[test]
    fn a_shared_shape_is_reached_through_a_recorded_transition() {
        let mut isolate = Isolate::new();
        let map0 = isolate.roots.js_object_map();

        let a = isolate.new_js_object();
        let a = Rooted::new(&mut isolate, a);
        isolate.set_property_by_str(a.get(&isolate), "x", Smi::from_int(1));
        let map1 = a.get(&isolate).map(&isolate.heap);

        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            TransitionsAccessor::search_transition(&isolate.heap, map0, x),
            Some(map1),
            "the edge must be cached on the source map"
        );

        let b = isolate.new_js_object();
        let b = Rooted::new(&mut isolate, b);
        isolate.set_property_by_str(b.get(&isolate), "x", Smi::from_int(9));
        assert_eq!(b.get(&isolate).map(&isolate.heap), map1);
    }

    #[test]
    fn a_branching_shape_graph_keeps_both_paths() {
        let mut isolate = Isolate::new();
        let base = isolate.new_js_object();
        let base = Rooted::new(&mut isolate, base);
        isolate.set_property_by_str(base.get(&isolate), "x", Smi::from_int(1));
        let map1 = base.get(&isolate).map(&isolate.heap);

        let left = isolate.new_js_object();
        let left = Rooted::new(&mut isolate, left);
        isolate.set_property_by_str(left.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(left.get(&isolate), "y", Smi::from_int(2));

        let right = isolate.new_js_object();
        let right = Rooted::new(&mut isolate, right);
        isolate.set_property_by_str(right.get(&isolate), "x", Smi::from_int(1));
        isolate.set_property_by_str(right.get(&isolate), "z", Smi::from_int(3));

        assert_ne!(
            left.get(&isolate).map(&isolate.heap),
            right.get(&isolate).map(&isolate.heap)
        );
        assert_eq!(
            TransitionsAccessor::number_of_transitions(&isolate.heap, map1),
            2,
            "one shape, two successors"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(left.get(&isolate), "y")),
            2
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(right.get(&isolate), "z")),
            3
        );
        assert_eq!(
            isolate.get_property_by_str(left.get(&isolate), "z"),
            isolate.undefined_value(),
            "shapes do not leak into each other"
        );
    }

    #[test]
    fn descriptor_arrays_are_shared_down_a_chain_rather_than_copied() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "a", Smi::from_int(1));
        let map1 = object.get(&isolate).map(&isolate.heap);
        let descriptors1 = map1.instance_descriptors(&isolate.heap);

        isolate.set_property_by_str(object.get(&isolate), "b", Smi::from_int(2));
        let map2 = object.get(&isolate).map(&isolate.heap);
        let descriptors2 = map2.instance_descriptors(&isolate.heap);

        assert_eq!(
            descriptors1, descriptors2,
            "the child should append into the parent's slack, not copy"
        );
        assert!(!map1.owns_descriptors(&isolate.heap));
        assert!(map2.owns_descriptors(&isolate.heap));
        assert_eq!(map1.number_of_own_descriptors(&isolate.heap), 1);
        assert_eq!(map2.number_of_own_descriptors(&isolate.heap), 2);
        let b = isolate.internalize_string("b").tagged();
        assert_eq!(
            map1.find_descriptor(&isolate.heap, b),
            K_NOT_FOUND,
            "the parent must not see the child's property"
        );
    }

    #[test]
    fn properties_spill_into_a_backing_store_when_the_inline_slots_run_out() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        let inline_slots = object
            .get(&isolate)
            .map(&isolate.heap)
            .inobject_properties(&isolate.heap);
        assert_eq!(inline_slots, Map::K_INITIAL_INOBJECT_SLACK);

        let count = inline_slots + 3;
        for i in 0..count {
            let name = format!("p{i}");
            assert!(isolate.set_property_by_str(object.get(&isolate), &name, Smi::from_int(i)));
        }

        for i in 0..count {
            let name = format!("p{i}");
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object.get(&isolate), &name)),
                i,
                "property {name} was lost"
            );
        }

        let array = object.get(&isolate).property_array(&isolate.heap);
        assert!(
            array.length(&isolate.heap) >= count - inline_slots,
            "the overflow must have a backing store"
        );
        assert_eq!(
            object
                .get(&isolate)
                .map(&isolate.heap)
                .number_of_own_descriptors(&isolate.heap),
            count
        );
    }

    #[test]
    fn overwriting_a_property_does_not_change_the_shape() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        let map = object.get(&isolate).map(&isolate.heap);

        assert!(isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(2)));
        assert_eq!(
            object.get(&isolate).map(&isolate.heap),
            map,
            "a write is not a shape change"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            2
        );
    }

    #[test]
    fn assigning_a_wider_value_generalizes_the_field_representation() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));

        let map = object.get(&isolate).map(&isolate.heap);
        let x = isolate.internalize_string("x").tagged();
        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Smi
        );

        let boxed = isolate.new_heap_number(1.5).tagged();
        assert!(isolate.set_property_by_str(object.get(&isolate), "x", boxed));

        assert_eq!(
            map.find_property_details(&isolate.heap, x)
                .unwrap()
                .representation(),
            Representation::Tagged,
            "a field that has held both a Smi and an object.get(&isolate) is tagged"
        );
        assert!(map.is_unstable(&isolate.heap));
        let read_back = isolate.get_property_by_str(object.get(&isolate), "x");
        assert_eq!(number_value(&isolate.heap, read_back), Some(1.5));
    }

    #[test]
    fn a_missing_property_reads_as_undefined() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        assert_eq!(
            isolate.get_property_by_str(object.get(&isolate), "nope"),
            isolate.undefined_value()
        );
        assert_eq!(
            isolate.get_own_property(object.get(&isolate), isolate.roots.empty_string),
            None
        );
    }

    #[test]
    fn lookup_walks_the_prototype_chain() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        let prototype = Rooted::new(&mut isolate, prototype);
        isolate.set_property_by_str(prototype.get(&isolate), "inherited", Smi::from_int(7));

        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "own", Smi::from_int(1));
        let map = object.get(&isolate).map(&isolate.heap);
        let prototype_value = prototype.get(&isolate).tagged();
        map.set_prototype(&mut isolate.heap, prototype_value);

        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "inherited")),
            7
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "own")),
            1
        );

        let inherited = isolate.internalize_string("inherited").tagged();
        assert_eq!(
            isolate.get_own_property(object.get(&isolate), inherited),
            None
        );

        let object_value = object.get(&isolate).tagged();
        let it = LookupIterator::new(&mut isolate, object_value, inherited);
        assert!(it.is_found());
        assert_eq!(it.get_holder(&isolate), prototype.get(&isolate));
        assert!(!it.holder_is_receiver(&isolate));
    }

    #[test]
    fn an_own_property_shadows_an_inherited_one() {
        let mut isolate = Isolate::new();
        let prototype = isolate.new_js_object();
        let prototype = Rooted::new(&mut isolate, prototype);
        isolate.set_property_by_str(prototype.get(&isolate), "x", Smi::from_int(7));

        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "marker", Smi::zero());
        let map = object.get(&isolate).map(&isolate.heap);
        let prototype_value = prototype.get(&isolate).tagged();
        map.set_prototype(&mut isolate.heap, prototype_value);

        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            7
        );
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            1,
            "the own property wins"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(prototype.get(&isolate), "x")),
            7,
            "and the prototype.get(&isolate) is untouched"
        );
    }

    #[test]
    fn a_non_extensible_object_refuses_new_properties_but_allows_writes() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(1));
        object
            .get(&isolate)
            .map(&isolate.heap)
            .set_is_extensible(&mut isolate.heap, false);

        assert!(
            !isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(2)),
            "Object.preventExtensions blocks additions"
        );
        assert!(
            isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(3)),
            "existing properties are still writable"
        );
        assert_eq!(
            Smi::to_int(isolate.get_property_by_str(object.get(&isolate), "x")),
            3
        );
    }

    #[test]
    fn a_long_shape_chain_stays_consistent() {
        let mut isolate = Isolate::new();
        let object = isolate.new_js_object();
        let object = Rooted::new(&mut isolate, object);
        isolate.handles.push(object.get(&isolate).tagged());
        const N: i32 = 40;
        for i in 0..N {
            assert!(isolate.set_property_by_str(
                object.get(&isolate),
                &format!("f{i}"),
                Smi::from_int(i * 3)
            ));
        }

        let map = object.get(&isolate).map(&isolate.heap);
        assert_eq!(map.number_of_own_descriptors(&isolate.heap), N);
        assert_eq!(map.own_property_names(&isolate.heap).len() as i32, N);

        for i in 0..N {
            assert_eq!(
                Smi::to_int(isolate.get_property_by_str(object.get(&isolate), &format!("f{i}"))),
                i * 3
            );
        }

        let mut steps = 0;
        let mut current = map;
        while let Some(parent) = current.back_pointer(&isolate.heap) {
            steps += 1;
            current = parent;
            assert!(
                steps <= N,
                "the back-pointer chain should be exactly N long"
            );
        }
        assert_eq!(steps, N);
        assert_eq!(current, isolate.roots.js_object_map());
    }

    #[test]
    fn a_thousand_identical_objects_produce_one_shape_chain() {
        let mut isolate = Isolate::new();
        let mut maps = std::collections::HashSet::new();
        for i in 0..1000 {
            let object = isolate.new_js_object();
            let object = Rooted::new(&mut isolate, object);
            isolate.set_property_by_str(object.get(&isolate), "x", Smi::from_int(i));
            isolate.set_property_by_str(object.get(&isolate), "y", Smi::from_int(i));
            maps.insert(object.get(&isolate).map(&isolate.heap));
        }
        assert_eq!(maps.len(), 1, "1000 objects, 1 hidden class");
    }
}

impl Isolate {
    #[inline]
    pub fn is_global_object(&self, object: JSObject) -> bool {
        self.heap.instance_type(object.object()) == instance_type::JS_GLOBAL_OBJECT_TYPE
    }

    pub fn global_cell(&self, name: Tagged) -> Option<crate::objects::property_cell::PropertyCell> {
        use crate::objects::property_cell::PropertyCell;
        let global = self.global_object();
        let map = global.map(&self.heap);
        let details = map.find_property_details(&self.heap, name)?;
        let value = global.fast_property_at(&self.heap, details.field_index() as i32);
        let object = value.to_strong_heap_object()?;
        if self.heap.instance_type(object) == instance_type::PROPERTY_CELL_TYPE {
            Some(PropertyCell::unchecked_cast(object))
        } else {
            None
        }
    }

    pub fn declare_global_cell(
        &mut self,
        name: Tagged,
        value: Tagged,
    ) -> crate::objects::property_cell::PropertyCell {
        if let Some(cell) = self.global_cell(name) {
            return cell;
        }
        let scope = self.handle_scope();
        let name_handle = self.create_handle(name);
        let cell = self.new_property_cell(name, value);
        let cell_handle = self.create_object_handle(cell.object());
        let name = name_handle.get(self);
        let cell_value = cell_handle.get(self);
        let global = self.global_object();
        self.set_property_ignoring_cells(global, name, cell_value, StoreOrigin::Named);
        let cell =
            crate::objects::property_cell::PropertyCell::unchecked_cast(cell_handle.object(self));
        self.close_handle_scope(scope);
        cell
    }

    pub fn set_global_cell_value(
        &mut self,
        cell: crate::objects::property_cell::PropertyCell,
        value: Tagged,
    ) {
        use crate::objects::property_cell::updated_cell_type;
        let updated = updated_cell_type(&self.heap, cell, value);
        let details = cell.property_details(&self.heap).with_cell_type(updated);
        cell.set_property_details(&mut self.heap, details);
        cell.set_value(&mut self.heap, value);
    }

    pub fn load_global(&self, name: Tagged) -> Option<Tagged> {
        Some(self.global_cell(name)?.value(&self.heap))
    }

    pub fn store_global(&mut self, name: Tagged, value: Tagged) {
        match self.global_cell(name) {
            Some(cell) => self.set_global_cell_value(cell, value),
            None => {
                self.declare_global_cell(name, value);
            }
        }
    }
}

impl Isolate {
    pub(crate) fn load_from_super(
        &mut self,
        receiver: Tagged,
        home_object: Tagged,
        name: Tagged,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        use crate::interpreter::interpreter::Thrown;
        let Some(home) = home_object.to_strong_heap_object() else {
            let message = self
                .new_string("Cannot read property of non-object home object")
                .tagged();
            return Err(Thrown(message));
        };
        let map = crate::objects::map::Map::unchecked_cast(self.heap.map(home));
        let start = map.prototype(&self.heap);
        let Some(holder) = start.to_strong_heap_object() else {
            return Ok(self.undefined_value());
        };
        if !crate::objects::instance_type::is_js_receiver(self.heap.instance_type(holder)) {
            return Ok(self.undefined_value());
        }
        self.get_property_with_receiver(JSObject::unchecked_cast(holder), name, receiver)
    }

    pub(crate) fn get_property_with_receiver(
        &mut self,
        start: JSObject,
        name: Tagged,
        receiver: Tagged,
    ) -> crate::interpreter::interpreter::ExecutionResult {
        let mut it = LookupIterator::with_lookup_start_object(
            self,
            receiver,
            name,
            start.tagged(),
            Configuration::PrototypeChain,
        );
        self.get_property(&mut it)
    }
}
