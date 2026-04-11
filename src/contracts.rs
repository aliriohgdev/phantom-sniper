use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function name() external view returns (string memory);
        function symbol() external view returns (string memory);
        function decimals() external view returns (uint8);
        function totalSupply() external view returns (uint256);
        function balanceOf(address owner) external view returns (uint256 balance);
        function transfer(address to, uint256 value) external returns (bool);
        function transferFrom(address from, address to, uint256 value) external returns (bool);
        function approve(address spender, uint256 value) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);

        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);
    }

    #[sol(rpc)]
    interface ITokenManager {
        function buyTokenAMAP(address token, uint256 funds, uint256 minAmount) external payable;
        function buyTokenAMAP(address token, address to, uint256 funds, uint256 minAmount) external payable;
        function sellToken(address token, uint256 amount, uint256 minFunds) external;
        function sellToken(address token, uint256 amount) external;
        function _tokenInfos(address token) external view returns (
            address base,
            address quote,
            uint256 template,
            uint256 totalSupply,
            uint256 maxOffers,
            uint256 maxRaising,
            uint256 launchTime,
            uint256 offers,
            uint256 funds,
            uint256 lastPrice,
            uint256 K,
            uint256 T,
            uint256 status
        );

        event TokenPurchase(
            address token,
            address account,
            uint256 price,
            uint256 amount,
            uint256 cost,
            uint256 fee,
            uint256 offers,
            uint256 funds
        );
        event TokenSale(
            address token,
            address account,
            uint256 price,
            uint256 amount,
            uint256 cost,
            uint256 fee,
            uint256 offers,
            uint256 funds
        );
        // Topic: 0x396d5e902b675b032348d3d2e9517ee8f0c4a926603fbc075d3d282ff00cad20
        event TokenCreate(
            address creator,
            address token,
            uint256 template,
            string name,
            string symbol,
            uint256 totalSupply,
            uint256 timestamp,
            uint256 virtualOffers
        );
    }
}
