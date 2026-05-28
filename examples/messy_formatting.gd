## This file demonstrates formatting issues that gdstyle fmt cleans up.
## Run: gdstyle fmt --diff examples/messy_formatting.gd

class_name MessyFormatting
extends Node

const PI_APPROX: float = 3.14
const HEX_COLOR = 0xFFAABB
const THRESHOLD = .5
const BIG_NUMBER = 1000000

var health: int = 100
var speed: float = .8
var name_tag: String = 'Player One'
var description: String = 'uses single quotes'

signal damage_taken
signal health_depleted

func _ready() -> void:
    var x = 10
    var y = 20
    var z = x+y*2
    if (x > 0):
        print("positive")
    if y != 0 && z < 100:
        print('non-zero')
    if !is_inside_tree() || !visible:
        return


func calculate(a: int, b: int, c: int) -> int:
    var result = a*b+c
    if (result > BIG_NUMBER):
        result = BIG_NUMBER
    return result



func update_health(amount: int) -> void:
    health+=amount
    if (health <= 0):
        health = 0
        health_depleted.emit()
    var msg = 'Health: ' + str(health);  print(msg)

func get_info() -> Dictionary:
    return {
        'name': name_tag,
        'health': health,
        'speed': speed,
        'hex': HEX_COLOR
    }

func apply_effects(effects: Array) -> void:
    for effect in effects:
        match effect:
            'heal':
                update_health(10)
            'damage':
                update_health(-10)
            'boost':
                speed+=.5
    var total = effects.size();  var applied = total
    print('Applied %d effects' % applied)
