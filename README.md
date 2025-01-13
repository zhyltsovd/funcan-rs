# funcan-rs

CANOpen implementation

## Project Status

This project is in the early stages of development. Features and documentation are subject to change.

## Contributing

Contributions are welcome! Please open issues or submit pull requests for any improvements or feature suggestions.

## License

This project is licensed under the MIT License. See the LICENSE file for more details.

## Contact

For further information, questions, or suggestions, feel free to reach out to me at zhyltsovd@gmail.com.

## TO-DO List

- x Base
  - x State machine trait
  - x Raw CAN frames
-   Core CANOpen functionalities
  -   Network Management 
    -   Implement NMT master functionalities
    -   Implement NMT slave functionalities
  -   Synchronization
    -   Implement SYNC producer
    -   Implement SYNC consumer
  -   SDO transfers
    -   Define SDO client behavior
    -   Define SDO server behavior
    -   Ensure segmented and expedited transfers
  -   PDO transfers
    -   Implement static PDO mapping
    -   Implement dynamic PDO mapping
    -   Support for asynchronous and synchronous PDOs
  -   Error handling
    -   Define error codes according to CANOpen standards
    -   CAN error management
    -   Implement emergency messages
