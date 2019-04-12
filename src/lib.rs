//!Learning Rust Wasm/WebAssembly with Virtual Dom Dodrio on a simple game for kids.

//region: Clippy
#![warn(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    //variable shadowing is idiomatic to Rust, but unnatural to me.
    clippy::shadow_reuse,
    clippy::shadow_same,
    clippy::shadow_unrelated,

)]
#![allow(
    //library from dependencies have this clippy warnings. Not my code.
    clippy::cargo_common_metadata,
    clippy::multiple_crate_versions,
    clippy::wildcard_dependencies,
    //Rust is more idiomatic without return statement
    clippy::implicit_return,
    //I have private function inside a function. Self does not work there.
    clippy::use_self,
    //Cannot add #[inline] to the start function with #[wasm_bindgen(start)]
    //because then wasm-pack build --target no-modules returns an error: export `run` not found 
    clippy::missing_inline_in_public_items
)]
//endregion

//region: use statements
use dodrio::bumpalo::{self, Bump};
use dodrio::{Node, Render};
use js_sys::Reflect;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use rand::FromEntropy;
use rand::Rng;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{
    console, Element, WebSocket,
};

extern crate console_error_panic_hook;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
//endregion

//region: enum, structs, const,...
///game title
const GAME_TITLE: &str = "mem2";
///fixed filename for card face down
const SRC_FOR_CARD_FACE_DOWN: &str = "content/img/mem_image_00_cardfacedown.png";

///Text of game rules.
///multiline string literal in Rust ends line with \
const GAME_RULES:& str = "The game starts with a grid of 8 randomly shuffled card pairs face down - 16 cards in all. \
The first player flips over two cards with two clicks. \
If the cards do not match, the next player will start his turn with a click to turn both cards back face down, then two clicks to flip over two card. \
If the cards match, they are left face up and the player receives a point and continues with the next turn. No additional third click needed in that case.";

///game description
const GAME_DESCRIPTION:& str = "This is a programming example for Rust Webassembly Virtual Dom application. \
For the sake of simplicity, it is made as for single player mode. \
The simple memory game is for kids. The images are funny cartoon characters from the alphabet. \
The cards grid is only 4x4.";

///Spelling for the alphabet - morse style
///the zero element is card face down or empty, alphabet begins with 01 : A
const SPELLING: [&str; 27] = [
    "", "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
    "juliet", "kilo", "lima", "mike", "november", "oscar", "papa", "quebec", "romeo", "sierra",
    "tango", "uniform", "victor", "whiskey", "xray", "yankee", "zulu",
];

///message struct for websocket
#[derive(Serialize, Deserialize)]
pub struct Message {
    ///user or player
    pub user: String,
    ///text message
    pub text: String,
}

///the 3 possible states of one card
enum CardStatusCardFace {
    ///card face down
    Down,
    ///card face Up Temporary
    UpTemporary,
    ///card face up Permanently
    UpPermanently,
}

///all the data for one card
struct Card {
    ///card status
    status: CardStatusCardFace,
    ///field for src attribute for HTML element imagea and filename of card image
    card_number_and_img_src: usize,
    ///field for id attribute for HTML element image contains the card index
    card_index_and_id: usize,
}

///the card grid struct has all the needed data for play logic and rendering
struct CardGridRootRenderingComponent {
    ///vector of cards
    vec_cards: Vec<Card>,
    //The player in one turn clicks 2 times and open 2 cards. If not match,
    //the third click closes opened cards and
    //it starts the next player turn.
    ///count click inside one turn
    count_click_inside_one_turn: u32,
    ///card index of first click
    card_index_of_first_click: usize,
    ///card index of second click
    card_index_of_second_click: usize,
    ///counts only clicks that flip the card. The third click is not counted.
    count_all_clicks: u32,
    ///web socket. used it to send message onclick.
    ws: WebSocket,
}
//endregion

//region: wasm_bindgen(start) is where everything starts
#[wasm_bindgen(start)]
///wasm_bindgen runs this functions at start
pub fn run() -> Result<(), JsValue> {
    // Initialize debugging for when/if something goes wrong.
    console_error_panic_hook::set_once();

    // Get the document's `<body>`.
    let window = web_sys::window().expect("error: web_sys::window");
    let document = window.document().expect("error: window.document");
    let virtual_dom_generated = document
        .get_element_by_id("virtual-dom-generated")
        .expect("No #virtual-dom-generated");

    let chat_display = document
        .get_element_by_id("chat-display")
        .expect("No #chat-display");
    let ws = setup_ws_connection();
    let ws_c = ws.clone();
    let template = document.create_element("div")?;
    
    // Construct a new `CardGrid` rendering component.
    let card_grid = CardGridRootRenderingComponent::new(ws_c);

    // Mount the component to the `<div id="virtual-dom-generated">`.
    let vdom = dodrio::Vdom::new(&virtual_dom_generated, card_grid);
    let vdom_weak = vdom.weak();
    setup_ws_msg_recv(&ws, chat_display, template, &vdom_weak);

    // Run the component forever.
    vdom.forget();

    Ok(())
}
//endregion

//region:CardGrid struct is the only persistant data we have in Rust Virtual Dom.dodrio
//in the constructor we initialize that data.
//Later onclick we change this data.
//at every animation frame we use only this data to render the virtual Dom.
impl CardGridRootRenderingComponent {
    /// Construct a new `CardGrid` component. Only once on the begining.
    pub fn new(ws: WebSocket) -> Self {
        //region: find 8 distinct random numbers between 1 and 26 for the alphabet cards
        //vec_of_random_numbers is 0 based
        let mut vec_of_random_numbers = Vec::new();
        let mut rng = SmallRng::from_entropy();
        let mut i = 0;
        while i < 8 {
            //gen_range is lower inclusive, upper exclusive 26 + 1
            let num: usize = rng.gen_range(1, 27);
            if dbg!(vec_of_random_numbers.contains(&num)) {
                //do nothing if the random number is repeated
                dbg!(num);
            } else {
                //push a pair of the same number
                vec_of_random_numbers.push(num);
                vec_of_random_numbers.push(num);
                i += 1;
            }
        }
        //endregion

        //region: shuffle the numbers
        let vrndslice = vec_of_random_numbers.as_mut_slice();
        vrndslice.shuffle(&mut rng);
        //endregion

        //region: create Cards from random numbers
        dbg!("vec_of_random_numbers values");
        let mut vec_card_from_random_numbers = Vec::new();

        //Index 0 is special and reserved for FaceDown. Cards start with base 1
        let new_card = Card {
            status: CardStatusCardFace::Down,
            card_number_and_img_src: 0,
            card_index_and_id: 0,
        };
        vec_card_from_random_numbers.push(new_card);

        //create the 16 card and push to the vector
        for (index, random_number) in vec_of_random_numbers.iter().enumerate() {
            let new_card = Card {
                status: CardStatusCardFace::Down,
                //dereference random number from iterator
                card_number_and_img_src: *random_number,
                //card base index will be 1. 0 is reserved for FaceDown.
                card_index_and_id: index.checked_add(1).expect("usize overflow"),
            };
            vec_card_from_random_numbers.push(new_card);
        }
        //endregion

        //region: return from constructor
        CardGridRootRenderingComponent {
            vec_cards: vec_card_from_random_numbers,
            count_click_inside_one_turn: 0,
            card_index_of_first_click: 0,
            card_index_of_second_click: 0,
            count_all_clicks: 0,
            ws: ws,
        }
        //endregion
    }
}
//endregion

//region: `Render` trait implementation on CardGrid struct
///It is called for every Dodrio animation frame to render the vdom.
///Probably only when something changes. Here it is a click on the cards.
///Not sure about that, but I don't see a reason to make execute it otherwise.
impl Render for CardGridRootRenderingComponent {
    #[inline]
    fn render<'a, 'bump>(&'a self, bump: &'bump Bump) -> Node<'bump>
    where
        'a: 'bump,
    {
        //local use statement, for this function only
        use dodrio::builder::*;

        //the card grid is a html css grid object (like a table) with <img> inside
        //other html elements are pretty simple.

        //region: private helper fn for Render()
        //here I use private functions for readability only, to avoid deep code nesting.
        //I don't understand closures enought to use them properly.
        //These private functions are not in the "impl Render forCardGrid" because of the error
        //method `from_card_number_to_img_src` is not a member of trait `Render`
        //there is not possible to write private and public methods in one impl block there are only pub methods.
        //`pub` not permitted there because it's implied
        //so I have to write functions outside of the impl block but inside my "module"

        ///format the src string
        fn from_card_number_to_img_src(bump: &Bump, card_number: usize) -> &str {
            bumpalo::format!(in bump, "content/img/mem_image_{:02}.png",card_number).into_bump_str()
        }

        ///The onclick event passed by javascript executes all the logic
        ///and changes only the fields of the Card Grid struct.
        ///That stuct is the only permanent data storage for later render the virtual dom.
        fn fn_on_click_code(card_grid: &mut CardGridRootRenderingComponent, this_click_card_index: usize) {
            //we have 3 possible clicks in one turn with different code branches.
            if card_grid.count_click_inside_one_turn >= 2 {
                //third click closes first and second card
                card_grid
                    .vec_cards
                    .get_mut(card_grid.card_index_of_first_click)
                    .expect("error card_grid.card_index_of_first_click ")
                    .status = CardStatusCardFace::Down;
                card_grid
                    .vec_cards
                    .get_mut(card_grid.card_index_of_second_click)
                    .expect("error card_grid.card_index_of_second_click")
                    .status = CardStatusCardFace::Down;
                card_grid.card_index_of_first_click = 0;
                card_grid.card_index_of_second_click = 0;
                card_grid.count_click_inside_one_turn = 0;
            } else if let CardStatusCardFace::Down = card_grid
                .vec_cards
                .get(this_click_card_index)
                .expect("error this_click_card_index")
                .status
            {
                card_grid
                    .vec_cards
                    .get_mut(this_click_card_index)
                    .expect("error this_click_card_index")
                    .status = CardStatusCardFace::UpTemporary;
                if card_grid.count_click_inside_one_turn == 0 {
                    //if is the first click, just count the clicks and open one card.
                    //before the first click reset the spelling.
                    //Usefull when there is no third click.
                    card_grid.card_index_of_first_click = 0;
                    card_grid.card_index_of_second_click = 0;
                    card_grid.card_index_of_first_click = this_click_card_index;
                    card_grid.count_click_inside_one_turn += 1;
                    card_grid.count_all_clicks += 1;
                } else {
                    //the only possibility is if card_grid.count_click_inside_one_turn == 1 {
                    //if is the second click, flip the card and then check for card match
                    card_grid.card_index_of_second_click = this_click_card_index;
                    card_grid.count_click_inside_one_turn += 1;
                    card_grid.count_all_clicks += 1;
                    //if the cards match, we don't need the third click
                    if card_grid
                        .vec_cards
                        .get_mut(card_grid.card_index_of_first_click)
                        .expect("error card_grid.card_index_of_first_click")
                        .card_number_and_img_src
                        == card_grid
                            .vec_cards
                            .get(card_grid.card_index_of_second_click)
                            .expect("error card_grid.card_index_of_second_click")
                            .card_number_and_img_src
                    {
                        // the two cards matches. make them permanent FaceUp
                        card_grid
                            .vec_cards
                            .get_mut(card_grid.card_index_of_first_click)
                            .expect("error card_grid.card_index_of_first_click")
                            .status = CardStatusCardFace::UpPermanently;
                        card_grid
                            .vec_cards
                            .get_mut(card_grid.card_index_of_second_click)
                            .expect("error card_grid.card_index_of_second_click")
                            .status = CardStatusCardFace::UpPermanently;
                        card_grid.count_click_inside_one_turn = 0;
                    }
                }
            }
        }

        ///prepare a vector<Node> for the Virtual Dom for grid item with <img>
        ///the grid container needs only grid items. There is no need for rows and columns in css grid.
        fn fn_vec_grid_item_bump<'a, 'bump>(
            cr_gr: &'a CardGridRootRenderingComponent,
            bump: &'bump Bump,
        ) -> Vec<Node<'bump>> {
            use dodrio::builder::*;
            let mut vec_grid_item_bump = Vec::new();
            for x in 1..=16 {
                let index: usize = x;
                let img_src = match cr_gr.vec_cards.get(index).expect("error index").status {
                    CardStatusCardFace::Down => SRC_FOR_CARD_FACE_DOWN,
                    CardStatusCardFace::UpTemporary | CardStatusCardFace::UpPermanently => {
                        from_card_number_to_img_src(
                            bump,
                            cr_gr
                                .vec_cards
                                .get(index)
                                .expect("error index")
                                .card_number_and_img_src,
                        )
                    }
                };
                // code for sound and opacity transition
                let onclick_sound_and_opacity_transition = if cr_gr.count_click_inside_one_turn <= 1
                {
                    //TODO: change audio from javascript to rust
                    //"this.style.opacity=1; var audio = new Audio('content/sound/mem_sound_{:02}.mp3');audio.play();",
                    bumpalo::format!(in bump,
                    "this.style.opacity=1;{}",
                    cr_gr.vec_cards.get(index).expect("error index").card_number_and_img_src
                    )
                    .into_bump_str()
                } else {
                    ""
                };

                //TODO: experiment transform from mutable to immutable with variable shadowing

                let img_id =
                    bumpalo::format!(in bump, "img{:02}",cr_gr.vec_cards.get(index).expect("error index").card_index_and_id)
                        .into_bump_str();

                let opacity = if img_src == SRC_FOR_CARD_FACE_DOWN {
                    bumpalo::format!(in bump, "opacity:{}", 0.2).into_bump_str()
                } else {
                    bumpalo::format!(in bump, "opacity:{}", 1).into_bump_str()
                };
                //creating 16 <div> in loop
                let grid_item_bump = div(bump)
                    .attr("class", "grid_item")
                    .children([img(bump)
                        .attr("src", img_src)
                        .attr("id", img_id)
                        .attr("style", opacity)
                        .attr("onclick", onclick_sound_and_opacity_transition)
                        //on click needs a code Closure in Rust. Dodrio and wasm-bindgen
                        //generate the javascript code to call it properly.
                        .on("click", move |root, vdom, event| {
                            // If the event's target is our image...
                            let img = match event
                                .target()
                                .and_then(|t| t.dyn_into::<web_sys::HtmlImageElement>().ok())
                            {
                                None => return,
                                //?? Don't understand what this does. The original was written for Input element.
                                Some(input) => input,
                            };
                            //we need our Struct CardGrid for Rust to write any data.
                            //It comes in the parameter root.
                            //All we can change is inside the struct CardGrid fields.
                            //The method render will later use that for rendering the new html.
                            let card_grid = root.unwrap_mut::<CardGridRootRenderingComponent>();

                            //id attribute of image html element is prefixed with img ex. "img12"
                            let this_click_card_index = (img.id().get(3..).expect("error slicing"))
                                .parse::<usize>()
                                .expect("error parse img id to usize");

                            card_grid
                                .ws
                                .send_with_str(
                                    &serde_json::to_string(&Message {
                                        user: "a".to_string(),
                                        text: format!("{}", this_click_card_index),
                                    })
                                    .expect("error sending test"),
                                )
                                .expect("Failed to send 'test' to server");

                            let audio_element = web_sys::HtmlAudioElement::new_with_src(
                                format!(
                                    "content/sound/mem_sound_{:02}.mp3",
                                    card_grid
                                        .vec_cards
                                        .get(this_click_card_index)
                                        .expect("error this_click_card_index")
                                        .card_number_and_img_src
                                )
                                .as_str(),
                            );
                            //unwrap is not an elegant way to deal with error, but is good enough for experimenting.
                            //play() return a Promise in JSValue. That is too hard for me to deal with now.
                            audio_element
                                .expect("Error: HtmlAudioElement new.")
                                .play()
                                .expect("Error: HtmlAudioElement.play() ");

                            fn_on_click_code(card_grid, this_click_card_index);
                            // Finally, re-render the component on the next animation frame.
                            vdom.schedule_render();
                        })
                        .finish()])
                    .finish();
                vec_grid_item_bump.push(grid_item_bump);
            }
            vec_grid_item_bump
        }

        ///the header can show only the game title or two spellings. Not everything together.
        ///I am trying to use simple closure this time, but I dont return the closure from the function.
        fn fn_grid_header<'a, 'bump>(cr_gr: &'a CardGridRootRenderingComponent, bump: &'bump Bump) -> Node<'bump> {
            use dodrio::builder::*;
            //if the Spellings are visible, than don't show GameTitle, because there is not
            //enought space on smartphones
            if cr_gr.card_index_of_first_click != 0 || cr_gr.card_index_of_second_click != 0 {
                //if the two opened card match use green else use red color
                let color; //haha variable does not need to be mutable. Great !

                if cr_gr
                    .vec_cards
                    .get(cr_gr.card_index_of_first_click)
                    .expect("error index")
                    .card_number_and_img_src
                    == cr_gr
                        .vec_cards
                        .get(cr_gr.card_index_of_second_click)
                        .expect("error index")
                        .card_number_and_img_src
                {
                    color = "green";
                } else if cr_gr.card_index_of_first_click == 0
                    || cr_gr.card_index_of_second_click == 0
                {
                    color = "yellow";
                } else {
                    color = "red";
                }

                {
                    //return
                    div(bump)
                .attr("class", "grid_container_header")
                .attr(
                    "style",
                    bumpalo::format!(in bump, "grid-template-columns: auto auto; color:{}",color)
                        .into_bump_str(),
                )
                .children([
                    div(bump)
                        .attr("class", "grid_item")
                        .attr("style", "text-align: left;")
                        .children([text(
                            SPELLING.get(cr_gr.vec_cards.get(cr_gr.card_index_of_first_click).expect("error index")
                                .card_number_and_img_src).expect("error index"),
                        )])
                        .finish(),
                    div(bump)
                        .attr("class", "grid_item")
                        .attr("style", "text-align: right;")
                        .children([text(
                            SPELLING.get(cr_gr.vec_cards.get(cr_gr.card_index_of_second_click).expect("error index")
                                .card_number_and_img_src).expect("error index"),
                        )])
                        .finish(),
                ])
                .finish()
                }
            } else {
                {
                    div(bump)
                        .attr("class", "grid_container_header")
                        .attr("style", "grid-template-columns: auto;")
                        .children([div(bump)
                            .attr("class", "grid_item")
                            .attr("style", "text-align: center;")
                            .children([text(GAME_TITLE)])
                            .finish()])
                        .finish()
                }
            }
        }
        //endregion

        //region: create the whole virtual dom. The verbose stuff is in private functions
        div(bump)
            .attr("class", "m_container")
            .children([
                fn_grid_header(self,bump),
                //div for the css grid object defined in css with <img> inside
                div(bump)
                    .attr("class", "grid_container")
                    .attr("style", "margin-left: auto;margin-right: auto;")
                    .children(fn_vec_grid_item_bump (self, bump ) )
                    .finish(),
                h3(bump)
                    .children([text(
                        bumpalo::format!(in bump, "Count of Clicks: {}", self.count_all_clicks)
                            .into_bump_str(),
                    )])
                    .finish(),
                h4(bump)
                    .children([text(GAME_DESCRIPTION)])
                    .finish(),
                h2(bump)
                    .children([text(
                        bumpalo::format!(in bump, "Memory game rules: {}", "").into_bump_str(),
                    )])
                    .finish(),
                h4(bump)
                    .children([text(GAME_RULES)])
                    .finish(),
                h6(bump)
                    .children([
                        text(bumpalo::format!(in bump, "Learning Rust programming: {}", "").into_bump_str(),),
                        a(bump)
                            .attr("href", "https://github.com/LucianoBestia/mem2")  
                            .attr("target","_blank")              
                            .children([text(bumpalo::format!(in bump, "https://github.com/LucianoBestia/mem2{}", "").into_bump_str(),)])
                            .finish(),
                    ])
                    .finish(),
            ])
            .finish()
        //endregion
    }
}
//endregion

//region: websocket communication
///setup connection
fn setup_ws_connection() -> WebSocket {
    let ws = WebSocket::new("ws://localhost:3012")
        .expect("WebSocket failed to connect 'ws://localhost:3012'");

    let ws_c = ws.clone();
    let open_handler = Box::new(move || {
        console::log_1(&"Connection opened, sending 'test' to server".into());
        ws_c.send_with_str(
            &serde_json::to_string(&Message {
                user: "connection".to_string(),
                text: "test".to_string(),
            })
            .expect("error sending test"),
        )
        .expect("Failed to send 'test' to server");
    });
    let cb_oh: Closure<Fn()> = Closure::wrap(open_handler);
    ws.set_onopen(Some(cb_oh.as_ref().unchecked_ref()));
    cb_oh.forget();
    ws
}
/// receive msg callback
/// TODO: write into Card Grid (root rendering element) of the vdom field, instead of html Element
fn setup_ws_msg_recv(ws: &WebSocket, msg_container: Element, template_node: Element, vdom: &dodrio::VdomWeak) {
    let msg_recv_handler = Box::new(move |msg: JsValue| {
        let data: JsValue =
            Reflect::get(&msg, &"data".into()).expect("No 'data' field in websocket message!");

        let message: Message =
            serde_json::from_str(&data.as_string().expect("Field 'data' is not string"))
                .unwrap_or_else(|x| Message {
                    user: "empty".to_string(),
                    text: x.to_string(),
                });

        let val = template_node
            .clone_node()
            .expect("Could not clone template node");
        let text = format!("{} says: {}", message.user, message.text);
        val.set_text_content(Some(&text));
        msg_container
            .append_child(&val)
            .expect("Could not append message node to container");
    });
    let cb_mrh: Closure<Fn(JsValue)> = Closure::wrap(msg_recv_handler);
    ws.set_onmessage(Some(cb_mrh.as_ref().unchecked_ref()));

    vdom.schedule_render();
    cb_mrh.forget();
}
//endregion
